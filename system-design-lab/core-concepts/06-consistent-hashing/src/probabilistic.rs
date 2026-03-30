#![allow(dead_code, unused_variables, unused_imports)]
//! # Probabilistic Hash Structures
//!
//! Hash-based streaming/sketching data structures for approximate counting
//! and similarity estimation.
//!
//! - **HyperLogLog**: estimate cardinality (distinct count) using O(1) memory
//! - **Count-Min Sketch**: estimate frequency of items in a stream
//! - **MinHash**: estimate Jaccard similarity between sets

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// =============================================================================
// HyperLogLog — Cardinality Estimation
// =============================================================================
// Estimates the number of distinct elements in a multiset using O(m) bytes
// where m is small (e.g., 2^14 = 16KB for ~0.8% error).
//
// Idea: hash each element, count leading zeros. More leading zeros observed
// → more elements likely present (probabilistic argument).
//
// Error rate ≈ 1.04 / sqrt(m) where m = number of registers.
//
// Used in: Redis PFCOUNT, database query optimizers, analytics pipelines.

pub struct HyperLogLog {
    registers: Vec<u8>,
    num_registers: usize, // m — must be power of 2
    register_bits: u32,   // log2(m) — bits used to select register
}

impl HyperLogLog {
    /// Create with given precision bits p. Uses 2^p registers.
    /// p=14 → 16K registers → ~0.8% error.
    pub fn new(precision_bits: u32) -> Self {
        let m = 1usize << precision_bits;
        Self {
            registers: vec![0u8; m],
            num_registers: m,
            register_bits: precision_bits,
        }
    }

    pub fn add<T: Hash>(&mut self, item: &T) {
        let hash = self.hash(item);

        // First p bits → register index
        let idx = (hash >> (64 - self.register_bits)) as usize;

        // Remaining bits → count leading zeros + 1
        let remaining = (hash << self.register_bits) | (1 << (self.register_bits - 1));
        let zeros = remaining.leading_zeros() as u8 + 1;

        self.registers[idx] = self.registers[idx].max(zeros);
    }

    /// Estimate the cardinality (distinct count).
    pub fn count(&self) -> f64 {
        let m = self.num_registers as f64;

        // Alpha_m correction constant
        let alpha = match self.num_registers {
            16 => 0.673,
            32 => 0.697,
            64 => 0.709,
            _ => 0.7213 / (1.0 + 1.079 / m),
        };

        // Harmonic mean of 2^(-register)
        let sum: f64 = self
            .registers
            .iter()
            .map(|&r| 2.0_f64.powi(-(r as i32)))
            .sum();

        let raw_estimate = alpha * m * m / sum;

        // Small range correction (linear counting)
        if raw_estimate <= 2.5 * m {
            let zeros = self.registers.iter().filter(|&&r| r == 0).count() as f64;
            if zeros > 0.0 {
                return m * (m / zeros).ln();
            }
        }

        raw_estimate
    }

    /// Merge another HyperLogLog into this one (for distributed counting).
    pub fn merge(&mut self, other: &HyperLogLog) {
        for (r, &o) in self.registers.iter_mut().zip(&other.registers) {
            *r = (*r).max(o);
        }
    }

    /// Memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.num_registers
    }

    fn hash<T: Hash>(&self, item: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        hasher.finish()
    }
}

// =============================================================================
// Count-Min Sketch — Frequency Estimation
// =============================================================================
// Estimates the frequency of items in a stream using O(w × d) counters.
// Always overestimates (never underestimates).
//
// Error bound: estimate ≤ true_count + ε·N with probability 1 - δ
// where w = ⌈e/ε⌉, d = ⌈ln(1/δ)⌉
//
// Used in: network traffic monitoring, NLP word counting, database query
// optimization, finding heavy hitters.

pub struct CountMinSketch {
    table: Vec<Vec<u32>>,
    width: usize,  // w — columns
    depth: usize,  // d — rows (hash functions)
    total: u64,
}

impl CountMinSketch {
    /// Create with given error bounds.
    /// epsilon: error factor (e.g., 0.001)
    /// delta: failure probability (e.g., 0.01)
    pub fn with_error(epsilon: f64, delta: f64) -> Self {
        let width = (std::f64::consts::E / epsilon).ceil() as usize;
        let depth = (1.0 / delta).ln().ceil() as usize;
        Self::new(width, depth)
    }

    pub fn new(width: usize, depth: usize) -> Self {
        Self {
            table: vec![vec![0u32; width]; depth],
            width,
            depth,
            total: 0,
        }
    }

    pub fn add<T: Hash>(&mut self, item: &T) {
        self.add_count(item, 1);
    }

    pub fn add_count<T: Hash>(&mut self, item: &T, count: u32) {
        for row in 0..self.depth {
            let col = self.hash(item, row);
            self.table[row][col] = self.table[row][col].saturating_add(count);
        }
        self.total += count as u64;
    }

    /// Estimate frequency — returns minimum across all hash rows.
    pub fn estimate<T: Hash>(&self, item: &T) -> u32 {
        (0..self.depth)
            .map(|row| {
                let col = self.hash(item, row);
                self.table[row][col]
            })
            .min()
            .unwrap_or(0)
    }

    fn hash<T: Hash>(&self, item: &T, seed: usize) -> usize {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        seed.hash(&mut hasher);
        (hasher.finish() as usize) % self.width
    }

    pub fn total_count(&self) -> u64 {
        self.total
    }

    /// Memory usage in bytes.
    pub fn memory_bytes(&self) -> usize {
        self.width * self.depth * 4
    }
}

// =============================================================================
// MinHash — Jaccard Similarity Estimation
// =============================================================================
// Estimates the Jaccard similarity J(A,B) = |A∩B| / |A∪B| between two sets
// using compact signatures.
//
// Uses k independent hash functions. For each set, stores the minimum hash
// value per function. Similarity ≈ fraction of matching minimums.
//
// Error ≈ 1/sqrt(k). k=200 → ~7% error.
//
// Used in: document deduplication, plagiarism detection, recommendation
// systems, near-duplicate web page detection, LSH for nearest neighbors.

pub struct MinHasher {
    num_hashes: usize,
    a_coeffs: Vec<u64>, // random coefficients for hash functions
    b_coeffs: Vec<u64>,
}

pub type MinHashSignature = Vec<u64>;

impl MinHasher {
    pub fn new(num_hashes: usize) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let a_coeffs: Vec<u64> = (0..num_hashes).map(|_| rng.gen::<u64>() | 1).collect(); // odd
        let b_coeffs: Vec<u64> = (0..num_hashes).map(|_| rng.gen()).collect();

        Self {
            num_hashes,
            a_coeffs,
            b_coeffs,
        }
    }

    /// Compute MinHash signature for a set of items.
    pub fn signature<T: Hash>(&self, items: &[T]) -> MinHashSignature {
        let mut mins = vec![u64::MAX; self.num_hashes];

        for item in items {
            let mut hasher = DefaultHasher::new();
            item.hash(&mut hasher);
            let h = hasher.finish();

            for i in 0..self.num_hashes {
                let hash_i = self.a_coeffs[i]
                    .wrapping_mul(h)
                    .wrapping_add(self.b_coeffs[i]);
                mins[i] = mins[i].min(hash_i);
            }
        }

        mins
    }

    /// Estimate Jaccard similarity from two signatures.
    pub fn similarity(sig_a: &MinHashSignature, sig_b: &MinHashSignature) -> f64 {
        let matches = sig_a
            .iter()
            .zip(sig_b)
            .filter(|(a, b)| a == b)
            .count();
        matches as f64 / sig_a.len() as f64
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== HyperLogLog (Cardinality Estimation) ===\n");
    let mut hll = HyperLogLog::new(14); // ~0.8% error, 16KB memory
    let n = 1_000_000;
    for i in 0..n {
        hll.add(&i);
    }
    let estimate = hll.count();
    let error = ((estimate - n as f64) / n as f64 * 100.0).abs();
    println!("  True cardinality: {n}");
    println!("  HLL estimate: {:.0}", estimate);
    println!("  Error: {:.2}%", error);
    println!("  Memory: {} bytes", hll.memory_bytes());

    // Merge test
    let mut hll_a = HyperLogLog::new(14);
    let mut hll_b = HyperLogLog::new(14);
    for i in 0..500_000 {
        hll_a.add(&i);
    }
    for i in 250_000..750_000 {
        hll_b.add(&i);
    }
    hll_a.merge(&hll_b);
    println!(
        "  Merged (0..500K ∪ 250K..750K): {:.0} (true: 750,000)",
        hll_a.count()
    );

    println!("\n=== Count-Min Sketch (Frequency Estimation) ===\n");
    let mut cms = CountMinSketch::with_error(0.001, 0.01); // ε=0.1%, δ=1%
    println!("  Table: {}×{} ({} bytes)", cms.width, cms.depth, cms.memory_bytes());

    // Simulate a stream with skewed frequencies
    for _ in 0..10_000 {
        cms.add(&"common");
    }
    for _ in 0..100 {
        cms.add(&"rare");
    }
    for _ in 0..1_000 {
        cms.add(&"medium");
    }

    println!("  Estimate 'common': {} (true: 10000)", cms.estimate(&"common"));
    println!("  Estimate 'rare':   {} (true: 100)", cms.estimate(&"rare"));
    println!("  Estimate 'medium': {} (true: 1000)", cms.estimate(&"medium"));
    println!("  Estimate 'absent': {} (true: 0)", cms.estimate(&"absent"));

    println!("\n=== MinHash (Jaccard Similarity) ===\n");
    let mh = MinHasher::new(200); // 200 hash functions → ~7% error

    let set_a: Vec<i32> = (0..1000).collect();
    let set_b: Vec<i32> = (500..1500).collect();     // 50% overlap with A
    let set_c: Vec<i32> = (0..1000).collect();        // identical to A
    let set_d: Vec<i32> = (10000..11000).collect();   // disjoint from A

    let sig_a = mh.signature(&set_a);
    let sig_b = mh.signature(&set_b);
    let sig_c = mh.signature(&set_c);
    let sig_d = mh.signature(&set_d);

    println!("  J(A, B) ≈ {:.3} (true: 0.333 — 500 overlap / 1500 union)",
        MinHasher::similarity(&sig_a, &sig_b));
    println!("  J(A, C) ≈ {:.3} (true: 1.000 — identical sets)",
        MinHasher::similarity(&sig_a, &sig_c));
    println!("  J(A, D) ≈ {:.3} (true: 0.000 — disjoint sets)",
        MinHasher::similarity(&sig_a, &sig_d));
}
