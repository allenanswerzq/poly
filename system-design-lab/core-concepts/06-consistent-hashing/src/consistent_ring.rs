#![allow(dead_code, unused_variables, unused_imports)]
//! # Consistent Hash Ring
//!
//! Production-quality consistent hash ring with:
//! - Virtual nodes for even key distribution
//! - O(log N) lookups using binary search on BTreeMap
//! - Support for weighted nodes
//! - Replication factor for redundancy
//!
//! When a node is added/removed, only ~K/N keys need to move
//! (K = total keys, N = number of nodes).

use std::collections::{BTreeMap, HashMap, HashSet};

/// Represents a position on the hash ring (0 to u32::MAX)
type RingPosition = u32;

/// Simple hash function
fn hash_key(key: &str) -> RingPosition {
    let digest = md5::compute(key.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
}

/// A consistent hash ring with virtual nodes
#[derive(Debug, Clone)]
pub struct ConsistentHashRing {
    /// Maps ring positions to node names
    ring: BTreeMap<RingPosition, String>,
    /// Number of virtual nodes per physical node
    virtual_nodes: usize,
    /// Track physical nodes and their virtual node count
    nodes: HashMap<String, usize>,
}

impl ConsistentHashRing {
    /// Create a new hash ring with specified virtual nodes per physical node
    ///
    /// More virtual nodes = better distribution but more memory
    /// Recommended: 100-200 for production systems
    pub fn new(virtual_nodes: usize) -> Self {
        Self {
            ring: BTreeMap::new(),
            virtual_nodes,
            nodes: HashMap::new(),
        }
    }

    /// Hash a string to a ring position using MD5
    fn hash(key: &str) -> RingPosition {
        hash_key(key)
    }

    /// Add a physical node to the ring
    pub fn add_node(&mut self, node: &str) {
        self.add_node_with_weight(node, 1);
    }

    /// Add a node with weight (more weight = more virtual nodes)
    pub fn add_node_with_weight(&mut self, node: &str, weight: usize) {
        let vnode_count = self.virtual_nodes * weight;

        for i in 0..vnode_count {
            let vnode_key = format!("{}#{}", node, i);
            let position = Self::hash(&vnode_key);
            self.ring.insert(position, node.to_string());
        }

        self.nodes.insert(node.to_string(), vnode_count);
        println!("Added node '{}' with {} virtual nodes", node, vnode_count);
    }

    /// Remove a physical node (and all its virtual nodes)
    pub fn remove_node(&mut self, node: &str) {
        if let Some(vnode_count) = self.nodes.remove(node) {
            for i in 0..vnode_count {
                let vnode_key = format!("{}#{}", node, i);
                let position = Self::hash(&vnode_key);
                self.ring.remove(&position);
            }
            println!("Removed node '{}' ({} virtual nodes)", node, vnode_count);
        }
    }

    /// Get the node responsible for a key
    ///
    /// Walks clockwise from the key's hash position to find the first node.
    pub fn get_node(&self, key: &str) -> Option<&str> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = Self::hash(key);

        // Find the first node at or after this position (clockwise)
        // If none found, wrap around to the first node
        match self.ring.range(hash..).next() {
            Some((_, node)) => Some(node),
            None => self.ring.values().next().map(|s| s.as_str()),
        }
    }

    /// Get N nodes for replication (consecutive distinct physical nodes on the ring)
    pub fn get_nodes(&self, key: &str, count: usize) -> Vec<&str> {
        if self.ring.is_empty() {
            return vec![];
        }

        let hash = Self::hash(key);
        let mut result = Vec::with_capacity(count);
        let mut seen = HashSet::new();

        // First pass: nodes after the hash position
        for (_, node) in self.ring.range(hash..) {
            if seen.insert(node) {
                result.push(node.as_str());
                if result.len() >= count {
                    return result;
                }
            }
        }

        // Wrap around: nodes from the beginning
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

    /// Get statistics about key distribution
    pub fn get_distribution(&self, sample_keys: &[String]) -> HashMap<String, usize> {
        let mut distribution = HashMap::new();

        for key in sample_keys {
            if let Some(node) = self.get_node(key) {
                *distribution.entry(node.to_string()).or_insert(0) += 1;
            }
        }

        distribution
    }

    /// Snapshot the key→node mapping for a set of keys.
    /// Used to compare before/after when the ring changes.
    pub fn snapshot(&self, keys: &[String]) -> Vec<String> {
        keys.iter()
            .map(|k| self.get_node(k).unwrap_or("").to_string())
            .collect()
    }

    /// Number of physical nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of virtual nodes on the ring
    pub fn ring_size(&self) -> usize {
        self.ring.len()
    }
}

/// Calculate how many keys moved by comparing before/after snapshots.
pub fn calculate_key_movement(before: &[String], after: &[String]) -> usize {
    before
        .iter()
        .zip(after.iter())
        .filter(|(old, new)| old != new)
        .count()
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Consistent Hash Ring ===\n");

    // Create a ring with 100 virtual nodes per physical node
    let mut ring = ConsistentHashRing::new(100);

    // Add initial nodes
    ring.add_node("cache-server-1");
    ring.add_node("cache-server-2");
    ring.add_node("cache-server-3");

    println!(
        "\nRing has {} physical nodes, {} virtual nodes\n",
        ring.node_count(),
        ring.ring_size()
    );

    // Generate sample keys
    let sample_keys: Vec<String> = (0..10000).map(|i| format!("user:{}", i)).collect();

    // Show initial distribution
    println!("--- Key Distribution (10,000 keys) ---");
    let dist = ring.get_distribution(&sample_keys);
    for (node, count) in &dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }

    // Show which node handles specific keys
    println!("\n--- Sample Key Lookups ---");
    for key in &[
        "user:123",
        "user:456",
        "user:789",
        "session:abc",
        "order:999",
    ] {
        let node = ring.get_node(key).unwrap();
        println!("  {} -> {}", key, node);
    }

    // Show replication
    println!("\n--- Replication Example ---");
    let replicas = ring.get_nodes("user:123", 3);
    println!("  Key 'user:123' replicated to: {:?}", replicas);

    // Snapshot before adding a node
    let before_add = ring.snapshot(&sample_keys);

    // Add a node
    println!("\n--- Adding cache-server-4 ---");
    ring.add_node("cache-server-4");

    let after_add = ring.snapshot(&sample_keys);
    let moved = calculate_key_movement(&before_add, &after_add);
    let move_pct = moved as f64 / sample_keys.len() as f64 * 100.0;

    let new_dist = ring.get_distribution(&sample_keys);
    println!("\nNew distribution:");
    for (node, count) in &new_dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }
    println!(
        "\nKeys moved: {} ({:.1}%) — theoretical 1/N = {:.1}%",
        moved,
        move_pct,
        100.0 / ring.node_count() as f64,
    );

    // Snapshot before removing a node
    let before_remove = ring.snapshot(&sample_keys);

    // Remove a node
    println!("\n--- Removing cache-server-2 ---");
    ring.remove_node("cache-server-2");

    let after_remove = ring.snapshot(&sample_keys);
    let moved = calculate_key_movement(&before_remove, &after_remove);
    let move_pct = moved as f64 / sample_keys.len() as f64 * 100.0;

    let final_dist = ring.get_distribution(&sample_keys);
    println!("\nFinal distribution:");
    for (node, count) in &final_dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }
    println!(
        "\nKeys moved: {} ({:.1}%) — ideally only {}'s keys redistributed",
        moved, move_pct, "cache-server-2",
    );

    // Weighted nodes
    println!("\n--- Weighted Nodes ---");
    let mut weighted = ConsistentHashRing::new(100);
    weighted.add_node_with_weight("small-server", 1);
    weighted.add_node_with_weight("large-server", 3);

    let wdist = weighted.get_distribution(&sample_keys);
    println!("\nWeighted distribution:");
    for (node, count) in &wdist {
        let pct = *count as f64 / sample_keys.len() as f64 * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, pct);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut ring = ConsistentHashRing::new(10);
        ring.add_node("node-a");
        ring.add_node("node-b");

        assert_eq!(ring.node_count(), 2);
        assert_eq!(ring.ring_size(), 20);
        assert!(ring.get_node("test-key").is_some());
    }

    #[test]
    fn test_consistent_lookup() {
        let mut ring = ConsistentHashRing::new(100);
        ring.add_node("node-a");
        ring.add_node("node-b");

        let node1 = ring.get_node("my-key");
        let node2 = ring.get_node("my-key");
        assert_eq!(node1, node2);
    }

    #[test]
    fn test_replication() {
        let mut ring = ConsistentHashRing::new(100);
        ring.add_node("node-a");
        ring.add_node("node-b");
        ring.add_node("node-c");

        let replicas = ring.get_nodes("key", 3);
        assert_eq!(replicas.len(), 3);

        let unique: HashSet<_> = replicas.iter().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn test_empty_ring() {
        let ring = ConsistentHashRing::new(100);
        assert!(ring.get_node("key").is_none());
        assert!(ring.get_nodes("key", 3).is_empty());
    }

    #[test]
    fn test_key_movement_add_node() {
        let mut ring = ConsistentHashRing::new(100);
        ring.add_node("a");
        ring.add_node("b");
        ring.add_node("c");

        let keys: Vec<String> = (0..10000).map(|i| format!("k:{i}")).collect();
        let before = ring.snapshot(&keys);

        ring.add_node("d");
        let after = ring.snapshot(&keys);

        let moved = calculate_key_movement(&before, &after);
        // With 4 nodes, ~25% should move. Allow some variance.
        let pct = moved as f64 / keys.len() as f64 * 100.0;
        assert!(pct > 15.0 && pct < 35.0, "moved {pct:.1}%, expected ~25%");
    }

    #[test]
    fn test_key_movement_remove_node() {
        let mut ring = ConsistentHashRing::new(100);
        ring.add_node("a");
        ring.add_node("b");
        ring.add_node("c");

        let keys: Vec<String> = (0..10000).map(|i| format!("k:{i}")).collect();
        let before = ring.snapshot(&keys);

        ring.remove_node("b");
        let after = ring.snapshot(&keys);

        let moved = calculate_key_movement(&before, &after);
        // Only b's keys should move (~33%)
        let pct = moved as f64 / keys.len() as f64 * 100.0;
        assert!(pct > 20.0 && pct < 45.0, "moved {pct:.1}%, expected ~33%");
    }
}
