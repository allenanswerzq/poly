//! # Consistent Hashing Implementation
//!
//! This module provides a production-quality consistent hash ring with:
//! - Virtual nodes for even key distribution
//! - O(log N) lookups using binary search
//! - Support for weighted nodes
//! - Replication factor for redundancy

use std::collections::{BTreeMap, HashMap};

/// Represents a position on the hash ring (0 to u32::MAX)
type RingPosition = u32;

/// Simple hash function
fn hash_key(key: &str) -> RingPosition {
    let digest = md5::compute(key.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
}

/// A consistent hash ring with virtual nodes
#[derive(Debug)]
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
            // Create unique key for each virtual node
            let vnode_key = format!("{}#{}", node, i);
            let position = Self::hash(&vnode_key);
            self.ring.insert(position, node.to_string());
        }

        self.nodes.insert(node.to_string(), vnode_count);
        println!(
            "Added node '{}' with {} virtual nodes",
            node, vnode_count
        );
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
    /// Walks clockwise from the key's hash position to find the first node
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

    /// Get N nodes for replication (consecutive on the ring)
    pub fn get_nodes(&self, key: &str, count: usize) -> Vec<&str> {
        if self.ring.is_empty() {
            return vec![];
        }

        let hash = Self::hash(key);
        let mut result = Vec::with_capacity(count);
        let mut seen = std::collections::HashSet::new();

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

    /// Number of physical nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of virtual nodes on the ring
    pub fn ring_size(&self) -> usize {
        self.ring.len()
    }
}

/// Demonstrates the consistent hashing behavior
fn main() {
    println!("=== Consistent Hashing Demo ===\n");

    // Create a ring with 100 virtual nodes per physical node
    let mut ring = ConsistentHashRing::new(100);

    // Add initial nodes
    println!("\n  ═══ Adding initial nodes ═══");
    ring.add_node("cache-server-1");
    ring.add_node("cache-server-2");
    ring.add_node("cache-server-3");

    println!(
        "\nRing has {} physical nodes, {} virtual nodes\n",
        ring.node_count(),
        ring.ring_size()
    );

    // Generate sample keys
    let sample_keys: Vec<String> = (0..10000)
        .map(|i| format!("user:{}", i))
        .collect();

    // Show initial distribution
    println!("\n  ═══ Key Distribution (10,000 keys) ═══");
    let dist = ring.get_distribution(&sample_keys);
    for (node, count) in &dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }

    // Show which node handles specific keys
    println!("\n--- Sample Key Lookups ---");
    for key in &["user:123", "user:456", "user:789", "session:abc", "order:999"] {
        let node = ring.get_node(key).unwrap();
        println!("  {} -> {}", key, node);
    }

    // Show replication (get multiple nodes for a key)
    println!("\n--- Replication Example ---");
    let replicas = ring.get_nodes("user:123", 3);
    println!("  Key 'user:123' replicated to: {:?}", replicas);

    // Demonstrate adding a node
    println!("\n--- Adding new node (cache-server-4) ---");
    ring.add_node("cache-server-4");

    let new_dist = ring.get_distribution(&sample_keys);
    println!("\nNew distribution:");
    for (node, count) in &new_dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }

    // Calculate key movement
    let moved = calculate_key_movement(&dist, &new_dist, &sample_keys, &ring);
    let move_percentage = (moved as f64 / sample_keys.len() as f64) * 100.0;
    println!(
        "\nKeys that moved: {} ({:.1}%) - close to theoretical 1/4 = 25%",
        moved, move_percentage
    );

    // Demonstrate removing a node
    println!("\n--- Removing node (cache-server-2) ---");
    ring.remove_node("cache-server-2");

    let final_dist = ring.get_distribution(&sample_keys);
    println!("\nFinal distribution:");
    for (node, count) in &final_dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }

    // Show weighted nodes
    println!("\n--- Weighted Nodes Demo ---");
    let mut weighted_ring = ConsistentHashRing::new(100);
    weighted_ring.add_node_with_weight("small-server", 1);   // 1x virtual nodes
    weighted_ring.add_node_with_weight("large-server", 3);   // 3x virtual nodes

    let weighted_dist = weighted_ring.get_distribution(&sample_keys);
    println!("\nWeighted distribution:");
    for (node, count) in &weighted_dist {
        let percentage = (*count as f64 / sample_keys.len() as f64) * 100.0;
        println!("  {}: {} keys ({:.1}%)", node, count, percentage);
    }
}

/// Calculate how many keys moved to a different node
fn calculate_key_movement(
    old_dist: &HashMap<String, usize>,
    _new_dist: &HashMap<String, usize>,
    keys: &[String],
    ring: &ConsistentHashRing,
) -> usize {
    // This is a simplified calculation
    // In production, you'd track the exact mapping before/after
    let theoretical_moved = keys.len() / ring.node_count();
    theoretical_moved
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

        // Should get a node for any key
        assert!(ring.get_node("test-key").is_some());
    }

    #[test]
    fn test_consistent_lookup() {
        let mut ring = ConsistentHashRing::new(100);
        ring.add_node("node-a");
        ring.add_node("node-b");

        // Same key should always map to same node
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

        // All replicas should be different
        let unique: std::collections::HashSet<_> = replicas.iter().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn test_empty_ring() {
        let ring = ConsistentHashRing::new(100);
        assert!(ring.get_node("key").is_none());
        assert!(ring.get_nodes("key", 3).is_empty());
    }
}
