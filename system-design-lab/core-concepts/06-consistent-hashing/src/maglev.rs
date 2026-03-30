#![allow(dead_code, unused_variables, unused_imports)]
//! # Maglev Hashing
//!
//! Google's consistent hashing for network load balancers (2016).
//! Builds a fixed-size lookup table providing O(1) lookups and minimal
//! disruption when backends change.
//!
//! Properties:
//! - O(1) lookup (table index)
//! - Near-perfect load balance
//! - Minimal disruption: changing backends remaps ~1/N entries
//! - Fixed memory: table size M (prime, typically 65537)
//!
//! Used in: Google Maglev load balancer, Cloudflare, Envoy proxy, Cilium.
//!
//! Paper: "Maglev: A Fast and Reliable Software Network Load Balancer"

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// =============================================================================
// Maglev Lookup Table
// =============================================================================

/// Default table size — must be a prime larger than max backends.
/// 65537 is commonly used in production.
const DEFAULT_TABLE_SIZE: usize = 65537;

pub struct MaglevHash {
    backends: Vec<String>,
    table: Vec<i32>, // -1 means empty, otherwise index into backends
    table_size: usize,
}

impl MaglevHash {
    pub fn new(backends: &[&str]) -> Self {
        Self::with_table_size(backends, DEFAULT_TABLE_SIZE)
    }

    pub fn with_table_size(backends: &[&str], table_size: usize) -> Self {
        let backend_names: Vec<String> = backends.iter().map(|s| s.to_string()).collect();
        let mut maglev = Self {
            backends: backend_names,
            table: vec![-1; table_size],
            table_size,
        };
        maglev.populate();
        maglev
    }

    /// Generate offset and skip for each backend.
    /// offset = hash1(name) % M
    /// skip   = hash2(name) % (M - 1) + 1  (must be non-zero for full coverage)
    fn permutation(name: &str, table_size: usize) -> (usize, usize) {
        let mut h1 = DefaultHasher::new();
        name.hash(&mut h1);
        let hash1 = h1.finish();

        let mut h2 = DefaultHasher::new();
        name.hash(&mut h2);
        h2.write_u8(0xFF); // different seed
        let hash2 = h2.finish();

        let offset = (hash1 % table_size as u64) as usize;
        let skip = (hash2 % (table_size as u64 - 1) + 1) as usize;
        (offset, skip)
    }

    /// Build the lookup table using Maglev's round-robin permutation fill.
    fn populate(&mut self) {
        let n = self.backends.len();
        if n == 0 {
            return;
        }

        // Calculate each backend's permutation sequence
        let perms: Vec<(usize, usize)> = self
            .backends
            .iter()
            .map(|name| Self::permutation(name, self.table_size))
            .collect();

        // Current position in each backend's permutation
        let mut next = vec![0usize; n];

        let mut filled = 0;
        // Round-robin: each backend claims slots in its permutation order
        'outer: loop {
            for i in 0..n {
                let (offset, skip) = perms[i];
                // Find next empty slot for backend i
                let mut c = (offset + next[i] * skip) % self.table_size;
                while self.table[c] >= 0 {
                    next[i] += 1;
                    c = (offset + next[i] * skip) % self.table_size;
                }
                self.table[c] = i as i32;
                next[i] += 1;
                filled += 1;
                if filled == self.table_size {
                    break 'outer;
                }
            }
        }
    }

    /// O(1) lookup: hash key → table index → backend.
    pub fn get_backend(&self, key: &str) -> Option<&str> {
        if self.backends.is_empty() {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % self.table_size;
        let backend_idx = self.table[idx] as usize;
        Some(&self.backends[backend_idx])
    }

    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }

    pub fn table_size(&self) -> usize {
        self.table_size
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Maglev Hashing ===\n");

    // Use smaller table for demo
    let backends = ["web-1", "web-2", "web-3", "web-4", "web-5"];
    let m = MaglevHash::with_table_size(&backends, 65537);

    // Lookups
    println!("Backend assignments:");
    for key in &["10.0.0.1:80", "10.0.0.2:443", "192.168.1.1:8080", "client-A", "client-B"] {
        println!("  {key} -> {}", m.get_backend(key).unwrap());
    }

    // Distribution
    let sample: Vec<String> = (0..50_000).map(|i| format!("conn:{i}")).collect();
    let mut dist = std::collections::HashMap::new();
    for key in &sample {
        let b = m.get_backend(key).unwrap();
        *dist.entry(b.to_string()).or_insert(0usize) += 1;
    }
    println!("\nDistribution (50,000 connections, 5 backends):");
    let mut sorted: Vec<_> = dist.iter().collect();
    sorted.sort_by_key(|(k, _)| k.clone());
    for (backend, count) in &sorted {
        println!("  {backend}: {count} ({:.1}%)", **count as f64 / sample.len() as f64 * 100.0);
    }

    // Disruption test: remove a backend
    let m_new = MaglevHash::with_table_size(&["web-1", "web-2", "web-4", "web-5"], 65537);
    let mut moved = 0;
    for key in &sample {
        let old = m.get_backend(key).unwrap();
        let new = m_new.get_backend(key).unwrap();
        if old != new {
            moved += 1;
        }
    }
    println!(
        "\nRemove web-3: {moved}/{} keys moved ({:.1}%) — ideal ~20%",
        sample.len(),
        moved as f64 / sample.len() as f64 * 100.0,
    );
}
