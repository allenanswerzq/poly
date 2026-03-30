#![allow(dead_code, unused_variables, unused_imports)]
//! # Hash Function Fundamentals
//!
//! Common non-cryptographic hash functions implemented from scratch,
//! with comparison of distribution, avalanche effect, and collision rates.
//!
//! Hash function properties:
//! - **Deterministic**: same input → same output
//! - **Uniform distribution**: outputs spread evenly across range
//! - **Avalanche effect**: small input change → ~50% of output bits flip
//! - **Fast**: O(n) in key length

// =============================================================================
// FNV-1a (Fowler-Noll-Vo)
// =============================================================================
// Fast, simple, good distribution. Used in Rust's default hasher.
// Processes byte-by-byte: XOR then multiply.

const FNV_OFFSET_32: u32 = 2166136261;
const FNV_PRIME_32: u32 = 16777619;
const FNV_OFFSET_64: u64 = 14695981039346656037;
const FNV_PRIME_64: u64 = 1099511628211;

pub fn fnv1a_32(data: &[u8]) -> u32 {
    let mut hash = FNV_OFFSET_32;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(FNV_PRIME_32);
    }
    hash
}

pub fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_64;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME_64);
    }
    hash
}

// =============================================================================
// DJB2 (Daniel J. Bernstein)
// =============================================================================
// One of the simplest and most widely known hash functions.
// hash = hash * 33 + byte

pub fn djb2(data: &[u8]) -> u32 {
    let mut hash: u32 = 5381;
    for &byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}

// =============================================================================
// MurmurHash3 (32-bit, finalization mix)
// =============================================================================
// Fast, excellent distribution, widely used in Hadoop, Cassandra, etc.
// This implements the 32-bit variant.

pub fn murmur3_32(data: &[u8], seed: u32) -> u32 {
    let c1: u32 = 0xcc9e2d51;
    let c2: u32 = 0x1b873593;

    let mut h1 = seed;
    let len = data.len();
    let nblocks = len / 4;

    // Body: process 4-byte blocks
    for i in 0..nblocks {
        let offset = i * 4;
        let mut k1 = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);

        k1 = k1.wrapping_mul(c1);
        k1 = k1.rotate_left(15);
        k1 = k1.wrapping_mul(c2);

        h1 ^= k1;
        h1 = h1.rotate_left(13);
        h1 = h1.wrapping_mul(5).wrapping_add(0xe6546b64);
    }

    // Tail: remaining bytes
    let tail = &data[nblocks * 4..];
    let mut k1: u32 = 0;
    match tail.len() {
        3 => {
            k1 ^= (tail[2] as u32) << 16;
            k1 ^= (tail[1] as u32) << 8;
            k1 ^= tail[0] as u32;
            k1 = k1.wrapping_mul(c1);
            k1 = k1.rotate_left(15);
            k1 = k1.wrapping_mul(c2);
            h1 ^= k1;
        }
        2 => {
            k1 ^= (tail[1] as u32) << 8;
            k1 ^= tail[0] as u32;
            k1 = k1.wrapping_mul(c1);
            k1 = k1.rotate_left(15);
            k1 = k1.wrapping_mul(c2);
            h1 ^= k1;
        }
        1 => {
            k1 ^= tail[0] as u32;
            k1 = k1.wrapping_mul(c1);
            k1 = k1.rotate_left(15);
            k1 = k1.wrapping_mul(c2);
            h1 ^= k1;
        }
        _ => {}
    }

    // Finalization
    h1 ^= len as u32;
    h1 = fmix32(h1);
    h1
}

fn fmix32(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85ebca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2ae35);
    h ^= h >> 16;
    h
}

// =============================================================================
// xxHash-style (simplified 32-bit)
// =============================================================================
// Extremely fast hash used in LZ4, Linux kernel, etc.

const XXHASH_PRIME1: u32 = 0x9E3779B1;
const XXHASH_PRIME2: u32 = 0x85EBCA77;
const XXHASH_PRIME3: u32 = 0xC2B2AE3D;
const XXHASH_PRIME4: u32 = 0x27D4EB2F;
const XXHASH_PRIME5: u32 = 0x165667B1;

pub fn xxhash32(data: &[u8], seed: u32) -> u32 {
    let len = data.len();
    let mut h32: u32;

    if len >= 16 {
        let mut v1 = seed.wrapping_add(XXHASH_PRIME1).wrapping_add(XXHASH_PRIME2);
        let mut v2 = seed.wrapping_add(XXHASH_PRIME2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(XXHASH_PRIME1);

        let blocks = len / 16;
        for i in 0..blocks {
            let off = i * 16;
            v1 = xxh32_round(v1, read_u32_le(data, off));
            v2 = xxh32_round(v2, read_u32_le(data, off + 4));
            v3 = xxh32_round(v3, read_u32_le(data, off + 8));
            v4 = xxh32_round(v4, read_u32_le(data, off + 12));
        }

        h32 = v1.rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
    } else {
        h32 = seed.wrapping_add(XXHASH_PRIME5);
    }

    h32 = h32.wrapping_add(len as u32);

    // Process remaining bytes
    let remaining_start = (len / 16) * 16;
    let mut idx = remaining_start;

    while idx + 4 <= len {
        h32 = h32.wrapping_add(read_u32_le(data, idx).wrapping_mul(XXHASH_PRIME3));
        h32 = h32.rotate_left(17).wrapping_mul(XXHASH_PRIME4);
        idx += 4;
    }
    while idx < len {
        h32 = h32.wrapping_add((data[idx] as u32).wrapping_mul(XXHASH_PRIME5));
        h32 = h32.rotate_left(11).wrapping_mul(XXHASH_PRIME1);
        idx += 1;
    }

    // Finalization
    h32 ^= h32 >> 15;
    h32 = h32.wrapping_mul(XXHASH_PRIME2);
    h32 ^= h32 >> 13;
    h32 = h32.wrapping_mul(XXHASH_PRIME3);
    h32 ^= h32 >> 16;
    h32
}

fn xxh32_round(acc: u32, input: u32) -> u32 {
    acc.wrapping_add(input.wrapping_mul(XXHASH_PRIME2))
        .rotate_left(13)
        .wrapping_mul(XXHASH_PRIME1)
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

// =============================================================================
// Avalanche Test
// =============================================================================
// Flip each bit of input, count how many output bits change.
// Ideal: 50% of output bits flip (for a 32-bit hash, ~16 bits).

pub fn avalanche_test<F: Fn(&[u8]) -> u32>(hash_fn: F, base_input: &[u8]) -> f64 {
    let base_hash = hash_fn(base_input);
    let mut total_flips = 0u64;
    let mut total_tests = 0u64;

    let mut modified = base_input.to_vec();

    for byte_idx in 0..modified.len() {
        for bit in 0..8 {
            modified[byte_idx] ^= 1 << bit;
            let new_hash = hash_fn(&modified);
            let diff = base_hash ^ new_hash;
            total_flips += diff.count_ones() as u64;
            total_tests += 1;
            modified[byte_idx] ^= 1 << bit; // restore
        }
    }

    total_flips as f64 / (total_tests as f64 * 32.0) * 100.0
}

// =============================================================================
// Collision Test
// =============================================================================

pub fn collision_test<F: Fn(&[u8]) -> u32>(hash_fn: F, num_keys: usize, bucket_bits: usize) -> f64 {
    use std::collections::HashSet;

    let num_buckets = 1usize << bucket_bits;
    let mut seen = HashSet::new();
    let mut collisions = 0;

    for i in 0..num_keys {
        let key = format!("key-{i}");
        let bucket = hash_fn(key.as_bytes()) as usize % num_buckets;
        if !seen.insert(bucket) {
            collisions += 1;
        }
    }

    collisions as f64 / num_keys as f64 * 100.0
}

// =============================================================================
// Distribution Test (Chi-squared)
// =============================================================================

pub fn distribution_test<F: Fn(&[u8]) -> u32>(hash_fn: F, num_keys: usize, num_buckets: usize) -> f64 {
    let mut buckets = vec![0usize; num_buckets];

    for i in 0..num_keys {
        let key = format!("item-{i}");
        let bucket = hash_fn(key.as_bytes()) as usize % num_buckets;
        buckets[bucket] += 1;
    }

    let expected = num_keys as f64 / num_buckets as f64;
    let chi_sq: f64 = buckets
        .iter()
        .map(|&count| {
            let diff = count as f64 - expected;
            diff * diff / expected
        })
        .sum();

    chi_sq / num_buckets as f64 // normalized, < 1.0 is good
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Hash Function Comparison ===\n");

    let test_inputs = [
        "hello",
        "hello!",
        "Hello",
        "consistent-hashing",
        "user:12345",
    ];

    println!("{:<24} {:>12} {:>12} {:>12} {:>12}",
        "Input", "FNV-1a", "DJB2", "Murmur3", "xxHash32");
    println!("{}", "-".repeat(76));

    for input in &test_inputs {
        let bytes = input.as_bytes();
        println!("{:<24} {:>12} {:>12} {:>12} {:>12}",
            input,
            format!("0x{:08x}", fnv1a_32(bytes)),
            format!("0x{:08x}", djb2(bytes)),
            format!("0x{:08x}", murmur3_32(bytes, 0)),
            format!("0x{:08x}", xxhash32(bytes, 0)),
        );
    }

    // Avalanche test
    println!("\n=== Avalanche Effect (ideal = 50%) ===");
    let base = b"test-input-for-avalanche";
    println!("  FNV-1a:   {:.1}%", avalanche_test(fnv1a_32, base));
    println!("  DJB2:     {:.1}%", avalanche_test(djb2, base));
    println!("  Murmur3:  {:.1}%", avalanche_test(|d| murmur3_32(d, 0), base));
    println!("  xxHash32: {:.1}%", avalanche_test(|d| xxhash32(d, 0), base));

    // Distribution test
    println!("\n=== Distribution Quality (chi-sq/buckets, lower = better) ===");
    let n = 100_000;
    let b = 1000;
    println!("  FNV-1a:   {:.4}", distribution_test(fnv1a_32, n, b));
    println!("  DJB2:     {:.4}", distribution_test(djb2, n, b));
    println!("  Murmur3:  {:.4}", distribution_test(|d| murmur3_32(d, 0), n, b));
    println!("  xxHash32: {:.4}", distribution_test(|d| xxhash32(d, 0), n, b));
}
