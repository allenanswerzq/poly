#![allow(dead_code, unused_variables, unused_imports)]
//! # Bloom Filter
//!
//! A space-efficient probabilistic data structure for membership testing.
//! - No false negatives: if it says "not in set", it's definitely not.
//! - Possible false positives: if it says "in set", it might not be.
//!
//! False positive rate ≈ (1 - e^(-kn/m))^k
//! where k = number of hash functions, n = elements inserted, m = bit array size.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub struct BloomFilter {
    bits: Vec<bool>,
    num_bits: usize,     // m
    num_hashes: usize,   // k
    count: usize,        // n (items inserted)
}

impl BloomFilter {
    /// Create a bloom filter optimized for `expected_items` with `false_positive_rate`.
    pub fn with_rate(expected_items: usize, false_positive_rate: f64) -> Self {
        // Optimal m = -n * ln(p) / (ln(2)^2)
        let m = (-(expected_items as f64) * false_positive_rate.ln() / (2.0_f64.ln().powi(2)))
            .ceil() as usize;
        // Optimal k = (m/n) * ln(2)
        let k = ((m as f64 / expected_items as f64) * 2.0_f64.ln()).ceil() as usize;
        Self::new(m.max(1), k.max(1))
    }

    pub fn new(num_bits: usize, num_hashes: usize) -> Self {
        Self {
            bits: vec![false; num_bits],
            num_bits,
            num_hashes,
            count: 0,
        }
    }

    /// Generate k hash positions using double hashing:
    /// h(i) = (h1 + i * h2) % m
    fn hash_positions<T: Hash>(&self, item: &T) -> Vec<usize> {
        let mut h1_hasher = DefaultHasher::new();
        item.hash(&mut h1_hasher);
        let h1 = h1_hasher.finish() as usize;

        // Second hash: seed with a different value
        let mut h2_hasher = DefaultHasher::new();
        item.hash(&mut h2_hasher);
        h2_hasher.write_u8(0x42);
        let h2 = h2_hasher.finish() as usize;

        (0..self.num_hashes)
            .map(|i| (h1.wrapping_add(i.wrapping_mul(h2))) % self.num_bits)
            .collect()
    }

    pub fn insert<T: Hash>(&mut self, item: &T) {
        for pos in self.hash_positions(item) {
            self.bits[pos] = true;
        }
        self.count += 1;
    }

    /// Check membership. `true` means "probably in set", `false` means "definitely not".
    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        self.hash_positions(item)
            .iter()
            .all(|&pos| self.bits[pos])
    }

    /// Estimated false positive rate given current fill.
    pub fn estimated_fp_rate(&self) -> f64 {
        let ones = self.bits.iter().filter(|&&b| b).count() as f64;
        let fill_ratio = ones / self.num_bits as f64;
        fill_ratio.powi(self.num_hashes as i32)
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn num_bits(&self) -> usize {
        self.num_bits
    }

    pub fn num_hashes(&self) -> usize {
        self.num_hashes
    }

    /// Memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.num_bits // Vec<bool> uses 1 byte per entry
    }
}

// =============================================================================
// Counting Bloom Filter (supports deletion)
// =============================================================================

pub struct CountingBloomFilter {
    counts: Vec<u8>,
    num_bits: usize,
    num_hashes: usize,
}

impl CountingBloomFilter {
    pub fn new(num_bits: usize, num_hashes: usize) -> Self {
        Self {
            counts: vec![0; num_bits],
            num_bits,
            num_hashes,
        }
    }

    fn hash_positions<T: Hash>(&self, item: &T) -> Vec<usize> {
        let mut h1_hasher = DefaultHasher::new();
        item.hash(&mut h1_hasher);
        let h1 = h1_hasher.finish() as usize;

        let mut h2_hasher = DefaultHasher::new();
        item.hash(&mut h2_hasher);
        h2_hasher.write_u8(0x42);
        let h2 = h2_hasher.finish() as usize;

        (0..self.num_hashes)
            .map(|i| (h1.wrapping_add(i.wrapping_mul(h2))) % self.num_bits)
            .collect()
    }

    pub fn insert<T: Hash>(&mut self, item: &T) {
        for pos in self.hash_positions(item) {
            self.counts[pos] = self.counts[pos].saturating_add(1);
        }
    }

    pub fn remove<T: Hash>(&mut self, item: &T) {
        // Only remove if the item is "probably" present
        if self.contains(item) {
            for pos in self.hash_positions(item) {
                self.counts[pos] = self.counts[pos].saturating_sub(1);
            }
        }
    }

    pub fn contains<T: Hash>(&self, item: &T) -> bool {
        self.hash_positions(item)
            .iter()
            .all(|&pos| self.counts[pos] > 0)
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Bloom Filter ===");
    // Target: 1000 items with 1% FP rate
    let mut bf = BloomFilter::with_rate(1000, 0.01);
    println!(
        "Config: {} bits ({} bytes), {} hashes for 1000 items @ 1% FP",
        bf.num_bits(),
        bf.memory_bytes(),
        bf.num_hashes()
    );

    // Insert some items
    for i in 0..1000 {
        bf.insert(&i);
    }

    // Check known members
    let mut found = 0;
    for i in 0..1000 {
        if bf.contains(&i) {
            found += 1;
        }
    }
    println!("True positives: {found}/1000 (should be 1000 — no false negatives)");

    // Check non-members for false positives
    let mut fp = 0;
    let test_range = 1000..2000;
    for i in test_range.clone() {
        if bf.contains(&i) {
            fp += 1;
        }
    }
    println!(
        "False positives: {fp}/{} ({:.1}%)",
        test_range.len(),
        fp as f64 / test_range.len() as f64 * 100.0
    );
    println!("Estimated FP rate: {:.2}%", bf.estimated_fp_rate() * 100.0);

    println!("\n=== Counting Bloom Filter (supports delete) ===");
    let mut cbf = CountingBloomFilter::new(1000, 5);
    cbf.insert(&"hello");
    cbf.insert(&"world");
    println!("contains 'hello': {}", cbf.contains(&"hello"));
    cbf.remove(&"hello");
    println!("after remove, contains 'hello': {}", cbf.contains(&"hello"));
    println!("contains 'world': {}", cbf.contains(&"world"));
}
