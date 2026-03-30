#![allow(dead_code, unused_variables, unused_imports)]
//! # Rendezvous Hashing (Highest Random Weight)
//!
//! Alternative to consistent hashing ring. Each key independently computes a
//! score for every node, picks the highest. Simpler and provably optimal key
//! redistribution — when a node is added/removed, only K/N keys move.
//!
//! Used in: Microsoft's Cache Array Routing Protocol (CARP), some CDNs,
//! GitHub's load balancer, distributed storage systems.
//!
//! Advantages over consistent hashing:
//! - No virtual nodes needed — naturally uniform
//! - Simpler implementation
//! - Easy weighted variant
//! - Perfect O(K/N) remapping guarantee
//!
//! Disadvantage: O(N) per lookup (must score all nodes).
//! Fine for N < 1000; for more, use a sorted subset.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// =============================================================================
// Rendezvous Hashing
// =============================================================================

pub struct RendezvousHash {
    nodes: Vec<(String, f64)>, // (name, weight)
}

impl RendezvousHash {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node(&mut self, name: &str) {
        self.add_node_weighted(name, 1.0);
    }

    pub fn add_node_weighted(&mut self, name: &str, weight: f64) {
        self.nodes.push((name.to_string(), weight));
    }

    pub fn remove_node(&mut self, name: &str) {
        self.nodes.retain(|(n, _)| n != name);
    }

    /// Compute score for (key, node) pair.
    /// Uses hash to generate a pseudo-random weight, adjusted by node weight.
    fn score(key: &str, node: &str, weight: f64) -> f64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        node.hash(&mut hasher);
        let hash = hasher.finish();

        // Convert hash to float in (0, 1) and apply weight
        // Using -weight / ln(hash_float) for weighted rendezvous
        let hash_float = (hash as f64) / (u64::MAX as f64);
        if hash_float == 0.0 {
            return f64::NEG_INFINITY;
        }
        weight / -hash_float.ln()
    }

    /// Get the node with highest score for this key — O(N).
    pub fn get_node(&self, key: &str) -> Option<&str> {
        self.nodes
            .iter()
            .max_by(|(name_a, w_a), (name_b, w_b)| {
                let score_a = Self::score(key, name_a, *w_a);
                let score_b = Self::score(key, name_b, *w_b);
                score_a.partial_cmp(&score_b).unwrap()
            })
            .map(|(name, _)| name.as_str())
    }

    /// Get top-N nodes for replication.
    pub fn get_nodes(&self, key: &str, count: usize) -> Vec<&str> {
        let mut scored: Vec<_> = self
            .nodes
            .iter()
            .map(|(name, weight)| {
                let score = Self::score(key, name, *weight);
                (name.as_str(), score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.iter().take(count).map(|(name, _)| *name).collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Rendezvous Hashing (HRW) ===\n");

    let mut rh = RendezvousHash::new();
    rh.add_node("node-A");
    rh.add_node("node-B");
    rh.add_node("node-C");
    rh.add_node("node-D");

    // Key lookups
    let keys = ["user:1", "user:2", "user:3", "user:100", "order:999"];
    println!("Key assignments (4 nodes):");
    for key in &keys {
        println!("  {key} -> {}", rh.get_node(key).unwrap());
    }

    // Replication
    println!("\nReplication for 'user:1': {:?}", rh.get_nodes("user:1", 3));

    // Distribution test
    let sample: Vec<String> = (0..10000).map(|i| format!("key:{i}")).collect();
    let mut dist = std::collections::HashMap::new();
    for key in &sample {
        let node = rh.get_node(key).unwrap();
        *dist.entry(node.to_string()).or_insert(0usize) += 1;
    }
    println!("\nDistribution (10,000 keys):");
    for (node, count) in &dist {
        println!("  {node}: {count} ({:.1}%)", *count as f64 / 100.0);
    }

    // Show minimal disruption: remove a node
    println!("\n--- Remove node-C ---");
    let old_assignments: Vec<_> = sample.iter().map(|k| rh.get_node(k).unwrap().to_string()).collect();
    rh.remove_node("node-C");
    let new_assignments: Vec<_> = sample.iter().map(|k| rh.get_node(k).unwrap().to_string()).collect();

    let moved = old_assignments
        .iter()
        .zip(&new_assignments)
        .filter(|(a, b)| a != b)
        .count();
    println!("Keys moved: {moved} ({:.1}%) — ideal ~25%", moved as f64 / 100.0);

    // Weighted nodes
    println!("\n--- Weighted Rendezvous ---");
    let mut wrh = RendezvousHash::new();
    wrh.add_node_weighted("small", 1.0);
    wrh.add_node_weighted("large", 3.0);

    let mut wdist = [0usize; 2];
    for key in &sample {
        match wrh.get_node(key).unwrap() {
            "small" => wdist[0] += 1,
            "large" => wdist[1] += 1,
            _ => {}
        }
    }
    println!("  small (weight=1): {} ({:.1}%)", wdist[0], wdist[0] as f64 / 100.0);
    println!("  large (weight=3): {} ({:.1}%)", wdist[1], wdist[1] as f64 / 100.0);
}
