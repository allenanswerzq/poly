#![allow(dead_code, unused_variables, unused_imports)]
//! # Distributed Cache Implementation
//!
//! Demonstrates a sharded distributed cache with:
//! - Consistent hashing for node distribution
//! - Replication for fault tolerance
//! - TTL support
//! - Cache-aside pattern

use dashmap::DashMap;
use parking_lot::RwLock;

fn hash_key(key: &str) -> u64 {
    let digest = md5::compute(key.as_bytes());
    u64::from_le_bytes(digest[0..8].try_into().unwrap())
}
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Cache Node
// =============================================================================

struct CacheEntry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| exp < Instant::now())
            .unwrap_or(false)
    }
}

/// A single cache node/shard
pub struct CacheNode {
    id: String,
    data: DashMap<String, CacheEntry>,
    max_size: usize,
}

impl CacheNode {
    pub fn new(id: &str, max_size: usize) -> Self {
        Self {
            id: id.to_string(),
            data: DashMap::new(),
            max_size,
        }
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).and_then(|entry| {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                None
            } else {
                Some(entry.value.clone())
            }
        })
    }

    pub fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) {
        // Simple eviction: if full, remove oldest
        if self.data.len() >= self.max_size {
            if let Some(key_to_remove) = self.data.iter().next().map(|e| e.key().clone()) {
                self.data.remove(&key_to_remove);
            }
        }

        let entry = CacheEntry {
            value,
            expires_at: ttl.map(|t| Instant::now() + t),
        };
        self.data.insert(key.to_string(), entry);
    }

    pub fn delete(&self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

// =============================================================================
// Consistent Hash Ring
// =============================================================================

type Position = u32;

struct ConsistentHashRing {
    ring: BTreeMap<Position, String>,
    virtual_nodes: usize,
}

impl ConsistentHashRing {
    fn new(virtual_nodes: usize) -> Self {
        Self {
            ring: BTreeMap::new(),
            virtual_nodes,
        }
    }

    fn hash(key: &str) -> Position {
        let digest = md5::compute(key.as_bytes());
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
    }

    fn add_node(&mut self, node_id: &str) {
        for i in 0..self.virtual_nodes {
            let vnode_key = format!("{}#{}", node_id, i);
            let pos = Self::hash(&vnode_key);
            self.ring.insert(pos, node_id.to_string());
        }
    }

    fn remove_node(&mut self, node_id: &str) {
        for i in 0..self.virtual_nodes {
            let vnode_key = format!("{}#{}", node_id, i);
            let pos = Self::hash(&vnode_key);
            self.ring.remove(&pos);
        }
    }

    fn get_node(&self, key: &str) -> Option<&str> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = Self::hash(key);
        match self.ring.range(hash..).next() {
            Some((_, node)) => Some(node),
            None => self.ring.values().next().map(|s| s.as_str()),
        }
    }

    /// Get N nodes for replication
    fn get_nodes(&self, key: &str, count: usize) -> Vec<&str> {
        if self.ring.is_empty() {
            return vec![];
        }

        let hash = Self::hash(key);
        let mut result = Vec::with_capacity(count);
        let mut seen = std::collections::HashSet::new();

        // Start from hash position
        for (_, node) in self.ring.range(hash..) {
            if seen.insert(node) {
                result.push(node.as_str());
                if result.len() >= count {
                    return result;
                }
            }
        }

        // Wrap around
        for (_, node) in self.ring.iter() {
            if seen.insert(node) {
                result.push(node.as_str());
                if result.len() >= count {
                    break;
                }
            }
        }

        result
    }
}

// =============================================================================
// Distributed Cache
// =============================================================================

/// Distributed cache with consistent hashing and replication
pub struct DistributedCache {
    nodes: DashMap<String, Arc<CacheNode>>,
    ring: RwLock<ConsistentHashRing>,
    replication_factor: usize,
}

impl DistributedCache {
    pub fn new(replication_factor: usize) -> Self {
        Self {
            nodes: DashMap::new(),
            ring: RwLock::new(ConsistentHashRing::new(100)),
            replication_factor,
        }
    }

    /// Add a cache node to the cluster
    pub fn add_node(&self, node_id: &str, max_size: usize) {
        let node = Arc::new(CacheNode::new(node_id, max_size));
        self.nodes.insert(node_id.to_string(), node);
        self.ring.write().add_node(node_id);
        println!("[Cluster] Added node: {}", node_id);
    }

    /// Remove a cache node from the cluster
    pub fn remove_node(&self, node_id: &str) {
        self.nodes.remove(node_id);
        self.ring.write().remove_node(node_id);
        println!("[Cluster] Removed node: {}", node_id);
    }

    /// Get a value (tries all replicas)
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let ring = self.ring.read();
        let node_ids = ring.get_nodes(key, self.replication_factor);

        for node_id in node_ids {
            if let Some(node) = self.nodes.get(node_id) {
                if let Some(value) = node.get(key) {
                    return Some(value);
                }
            }
        }

        None
    }

    /// Set a value (writes to all replicas)
    pub fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) {
        let ring = self.ring.read();
        let node_ids = ring.get_nodes(key, self.replication_factor);

        for node_id in node_ids {
            if let Some(node) = self.nodes.get(node_id) {
                node.set(key, value.clone(), ttl);
            }
        }
    }

    /// Delete a value from all replicas
    pub fn delete(&self, key: &str) -> bool {
        let ring = self.ring.read();
        let node_ids = ring.get_nodes(key, self.replication_factor);
        let mut deleted = false;

        for node_id in node_ids {
            if let Some(node) = self.nodes.get(node_id) {
                if node.delete(key) {
                    deleted = true;
                }
            }
        }

        deleted
    }

    /// Get placement info for a key
    pub fn get_placement(&self, key: &str) -> Vec<String> {
        let ring = self.ring.read();
        ring.get_nodes(key, self.replication_factor)
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Get cluster stats
    pub fn stats(&self) -> Vec<(String, usize)> {
        self.nodes
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().size()))
            .collect()
    }
}

// =============================================================================
// Cache Client with Cache-Aside Pattern
// =============================================================================

/// Simulates a database
pub struct Database {
    data: DashMap<String, String>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        // Simulate slow database query
        std::thread::sleep(Duration::from_millis(10));
        self.data.get(key).map(|v| v.clone())
    }

    pub fn set(&self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache client implementing cache-aside pattern
pub struct CacheClient {
    cache: Arc<DistributedCache>,
    db: Arc<Database>,
    ttl: Duration,
}

impl CacheClient {
    pub fn new(cache: Arc<DistributedCache>, db: Arc<Database>, ttl: Duration) -> Self {
        Self { cache, db, ttl }
    }

    /// Get with cache-aside pattern
    pub fn get(&self, key: &str) -> Option<String> {
        // Try cache first
        if let Some(bytes) = self.cache.get(key) {
            println!("  [Cache HIT] {}", key);
            return Some(String::from_utf8_lossy(&bytes).to_string());
        }

        // Cache miss - query database
        println!("  [Cache MISS] {} - querying DB", key);
        if let Some(value) = self.db.get(key) {
            // Store in cache
            self.cache
                .set(key, value.as_bytes().to_vec(), Some(self.ttl));
            Some(value)
        } else {
            None
        }
    }

    /// Set with write-through
    pub fn set(&self, key: &str, value: &str) {
        // Write to database first
        self.db.set(key, value);
        // Then update cache
        self.cache
            .set(key, value.as_bytes().to_vec(), Some(self.ttl));
    }

    /// Delete with cache invalidation
    pub fn delete(&self, key: &str) {
        // Invalidate cache first (to avoid stale reads)
        self.cache.delete(key);
        // Then delete from database
        // db.delete(key);
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Distributed Cache Demo ===\n");

    // Create distributed cache with replication factor of 2
    let cache = Arc::new(DistributedCache::new(2));

    // Add cache nodes
    println!("\n  ═══ Adding Cache Nodes ═══");
    cache.add_node("cache-node-1", 1000);
    cache.add_node("cache-node-2", 1000);
    cache.add_node("cache-node-3", 1000);

    // Show key placement
    println!("\n--- Key Placement (with replication) ---");
    for key in &["user:1", "user:2", "user:3", "session:abc", "product:100"] {
        let nodes = cache.get_placement(key);
        println!("  {} -> {:?}", key, nodes);
    }

    // Set and get values
    println!("\n--- Basic Operations ---");
    cache.set("user:1", b"Alice".to_vec(), None);
    cache.set("user:2", b"Bob".to_vec(), None);

    println!(
        "GET user:1 = {:?}",
        cache
            .get("user:1")
            .map(|b| String::from_utf8_lossy(&b).to_string())
    );
    println!(
        "GET user:2 = {:?}",
        cache
            .get("user:2")
            .map(|b| String::from_utf8_lossy(&b).to_string())
    );
    println!("GET user:3 = {:?}", cache.get("user:3"));

    // Demonstrate fault tolerance
    println!("\n--- Fault Tolerance ---");
    println!("Removing cache-node-1...");
    cache.remove_node("cache-node-1");

    // Data should still be available from replica
    println!(
        "GET user:1 (should work from replica) = {:?}",
        cache
            .get("user:1")
            .map(|b| String::from_utf8_lossy(&b).to_string())
    );

    // Show cluster stats
    println!("\n--- Cluster Stats ---");
    for (node, size) in cache.stats() {
        println!("  {}: {} entries", node, size);
    }

    // Cache-aside pattern demo
    println!("\n--- Cache-Aside Pattern ---");
    let db = Arc::new(Database::new());

    // Pre-populate database
    db.set("product:1", "Laptop - $999");
    db.set("product:2", "Phone - $599");
    db.set("product:3", "Tablet - $399");

    let cache2 = Arc::new(DistributedCache::new(1));
    cache2.add_node("node-1", 100);

    let client = CacheClient::new(cache2, db, Duration::from_secs(60));

    println!("\nFirst access (cache miss, queries DB):");
    client.get("product:1");
    client.get("product:2");

    println!("\nSecond access (cache hit):");
    client.get("product:1");
    client.get("product:2");

    println!("\nAccess non-existent key:");
    let result = client.get("product:999");
    println!("  Result: {:?}", result);

    // TTL demo
    println!("\n--- TTL Demo ---");
    let cache3 = Arc::new(DistributedCache::new(1));
    cache3.add_node("node-1", 100);

    cache3.set("temp", b"data".to_vec(), Some(Duration::from_millis(100)));
    println!("Set 'temp' with 100ms TTL");
    println!(
        "GET temp (immediately) = {:?}",
        cache3.get("temp").is_some()
    );

    std::thread::sleep(Duration::from_millis(150));
    println!(
        "GET temp (after 150ms) = {:?}",
        cache3.get("temp").is_some()
    );

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let cache = DistributedCache::new(1);
        cache.add_node("node-1", 100);

        cache.set("key", b"value".to_vec(), None);
        assert_eq!(cache.get("key"), Some(b"value".to_vec()));

        cache.delete("key");
        assert_eq!(cache.get("key"), None);
    }

    #[test]
    fn test_replication() {
        let cache = DistributedCache::new(2);
        cache.add_node("node-1", 100);
        cache.add_node("node-2", 100);

        cache.set("key", b"value".to_vec(), None);
        assert_eq!(cache.get("key"), Some(b"value".to_vec()));

        // Remove one node - should still work
        cache.remove_node("node-1");
        assert_eq!(cache.get("key"), Some(b"value".to_vec()));
    }

    #[test]
    fn test_ttl() {
        let cache = DistributedCache::new(1);
        cache.add_node("node-1", 100);

        cache.set("key", b"value".to_vec(), Some(Duration::from_millis(50)));
        assert!(cache.get("key").is_some());

        std::thread::sleep(Duration::from_millis(60));
        assert!(cache.get("key").is_none());
    }
}
