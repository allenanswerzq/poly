#![allow(dead_code, unused_variables, unused_imports)]
//! # Jump Consistent Hash
//!
//! Google's algorithm (2014). Maps a key to one of N buckets using only
//! ~O(ln N) operations with perfect uniformity and minimal disruption.
//!
//! Properties:
//! - No memory overhead (no ring, no virtual nodes)
//! - Perfect balance: exactly 1/N keys per bucket
//! - Minimal remapping: changing N→N+1 moves exactly 1/(N+1) keys
//! - BUT: only supports adding/removing the last bucket (sequential numbering)
//!
//! Used in: Google's distributed storage internals, hash partitioning where
//! numbered buckets suffice. Combine with a node list for named nodes.
//!
//! Paper: "A Fast, Minimal Memory, Consistent Hash Algorithm" — Lamping & Veach

// =============================================================================
// Jump Consistent Hash (core algorithm)
// =============================================================================

/// Maps key to a bucket in [0, num_buckets) using jump consistent hash.
/// Time: O(ln(num_buckets)), Space: O(1).
pub fn jump_hash(mut key: u64, num_buckets: u32) -> u32 {
    let mut b: i64 = -1;
    let mut j: i64 = 0;

    while j < num_buckets as i64 {
        b = j;
        key = key.wrapping_mul(2862933555777941757).wrapping_add(1);
        j = ((b + 1) as f64 * ((1i64 << 31) as f64 / ((key >> 33) + 1) as f64)) as i64;
    }

    b as u32
}

// =============================================================================
// Named-node wrapper (maps bucket numbers to named servers)
// =============================================================================

pub struct JumpHashRing {
    nodes: Vec<String>,
}

impl JumpHashRing {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Add a node (appended to end — order matters for jump hash).
    pub fn add_node(&mut self, name: &str) {
        self.nodes.push(name.to_string());
    }

    /// Remove the last node (jump hash only supports removing from the end).
    pub fn remove_last(&mut self) -> Option<String> {
        self.nodes.pop()
    }

    pub fn get_node(&self, key: &str) -> Option<&str> {
        if self.nodes.is_empty() {
            return None;
        }
        let hash = hash_key(key);
        let bucket = jump_hash(hash, self.nodes.len() as u32) as usize;
        Some(&self.nodes[bucket])
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

fn hash_key(key: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    h.finish()
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Jump Consistent Hash ===\n");

    // Raw jump hash
    println!("Jump hash examples (10 buckets):");
    for i in 0..10u64 {
        println!("  key {i} -> bucket {}", jump_hash(i, 10));
    }

    // Distribution test
    let n = 100_000;
    let buckets = 5u32;
    let mut counts = vec![0usize; buckets as usize];
    for i in 0..n {
        let b = jump_hash(i, buckets) as usize;
        counts[b] += 1;
    }
    println!("\nDistribution ({n} keys, {buckets} buckets):");
    for (i, c) in counts.iter().enumerate() {
        println!("  bucket {i}: {c} ({:.1}%)", *c as f64 / n as f64 * 100.0);
    }

    // Minimal disruption test: 5 → 6 buckets
    let mut moved = 0;
    for i in 0..n {
        if jump_hash(i, 5) != jump_hash(i, 6) {
            moved += 1;
        }
    }
    println!(
        "\n5→6 buckets: {moved}/{n} keys moved ({:.1}%) — ideal {:.1}%",
        moved as f64 / n as f64 * 100.0,
        100.0 / 6.0
    );

    // Named nodes
    println!("\n--- Named Jump Hash Ring ---");
    let mut ring = JumpHashRing::new();
    ring.add_node("cache-1");
    ring.add_node("cache-2");
    ring.add_node("cache-3");

    for key in &["user:1", "user:2", "user:100", "order:42"] {
        println!("  {key} -> {}", ring.get_node(key).unwrap());
    }

    // Add a node and show minimal movement
    let sample: Vec<String> = (0..10000).map(|i| format!("k:{i}")).collect();
    let before: Vec<_> = sample.iter().map(|k| ring.get_node(k).unwrap().to_string()).collect();
    ring.add_node("cache-4");
    let after: Vec<_> = sample.iter().map(|k| ring.get_node(k).unwrap().to_string()).collect();
    let moved = before.iter().zip(&after).filter(|(a, b)| a != b).count();
    println!(
        "\nAdded cache-4: {moved}/10000 keys moved ({:.1}%) — ideal 25%",
        moved as f64 / 100.0
    );
}
