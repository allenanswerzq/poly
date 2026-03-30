#![allow(dead_code, unused_variables, unused_imports)]
//! # Hash Function Fundamentals
//!
//! Two families of hash functions, both implemented and compared:
//!
//! **Non-cryptographic** (fast, for hash tables / partitioning):
//!   FNV-1a, DJB2, MurmurHash3, xxHash
//!
//! **Cryptographic** (slower, for security / integrity):
//!   MD5, SHA-1, SHA-256
//!
//! Key properties every hash function needs:
//! - **Deterministic**: same input → same output
//! - **Uniform distribution**: outputs spread evenly across range
//! - **Avalanche effect**: small input change → ~50% of output bits flip
//!
//! Cryptographic hashes additionally need:
//! - **Pre-image resistance**: given H(x), can't find x
//! - **Collision resistance**: can't find two inputs with same output
//!
//! ┌───────────────────────────────────────────────────────────────────┐
//! │              NON-CRYPTOGRAPHIC vs CRYPTOGRAPHIC                   │
//! ├──────────────────┬────────────────────┬──────────────────────────┤
//! │                  │ Non-crypto         │ Crypto                   │
//! │                  │ (FNV, Murmur, xx)  │ (MD5, SHA-1, SHA-256)    │
//! ├──────────────────┼────────────────────┼──────────────────────────┤
//! │ Speed            │ ~5-10 GB/s         │ ~0.2-1 GB/s             │
//! │ Output size      │ 32-128 bits        │ 128-512 bits             │
//! │ Collision resist │ No guarantees      │ Designed for it          │
//! │ Use for          │ Hash tables, bloom │ Passwords, signatures,   │
//! │                  │ filters, sharding  │ TLS, git, blockchain     │
//! │ DON'T use for    │ Security!          │ Hash tables (too slow)   │
//! └──────────────────┴────────────────────┴──────────────────────────┘

use sha2::{Digest, Sha256};

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
// MD5 (Message Digest 5) — 128-bit cryptographic hash
// =============================================================================
// Designed 1991 by Ron Rivest. Produces a 128-bit (16-byte) digest.
// BROKEN for security (collisions found in 2004), but still widely used for:
//   - File checksums (md5sum)
//   - Cache keys
//   - Non-security fingerprinting
//
// We use the `md5` crate here (implementing MD5 from scratch is ~150 lines
// of bit manipulation — the algorithm uses 4 rounds of 16 operations each,
// processing 512-bit blocks with bitwise ops and sine-derived constants).

pub fn md5_hash(data: &[u8]) -> [u8; 16] {
    let digest = md5::compute(data);
    (*digest).into()
}

/// Returns first 8 bytes as u64 for comparison purposes.
pub fn md5_u64(data: &[u8]) -> u64 {
    let digest = md5::compute(data);
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3],
        digest[4], digest[5], digest[6], digest[7],
    ])
}

pub fn md5_hex(data: &[u8]) -> String {
    let digest = md5::compute(data);
    format!("{:032x}", digest)
}

// =============================================================================
// SHA-1 (Secure Hash Algorithm 1) — 160-bit
// =============================================================================
// Designed by NSA, published 1995. Produces 160-bit (20-byte) digest.
// BROKEN for security (Google demonstrated collision in 2017, "SHAttered").
// Still used in: Git commit hashes, some legacy TLS, HMAC-SHA1 (still OK).
//
// We implement the core algorithm from scratch:
// - Pads message to 512-bit blocks
// - 80 rounds of mixing per block using rotate/XOR/add
// - 5 state variables (H0-H4) updated each block

pub fn sha1(data: &[u8]) -> [u8; 20] {
    // Initial hash values
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80); // append bit '1'
    while msg.len() % 64 != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process 512-bit (64-byte) blocks
    for block in msg.chunks(64) {
        // Prepare 80-word message schedule
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);

        // 80 rounds
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

pub fn sha1_hex(data: &[u8]) -> String {
    sha1(data).iter().map(|b| format!("{b:02x}")).collect()
}

// =============================================================================
// SHA-256 (Secure Hash Algorithm 2) — 256-bit
// =============================================================================
// The current standard cryptographic hash. Secure as of 2026.
// Produces 256-bit (32-byte) digest. Used in:
//   - TLS/SSL certificates
//   - Bitcoin mining & Merkle trees
//   - Git (migrating from SHA-1)
//   - Digital signatures
//   - Password hashing (as part of bcrypt/scrypt/argon2)
//   - HMAC for API authentication
//
// We use the `sha2` crate (implementing SHA-256 from scratch is ~200 lines;
// same Merkle-Damgård structure as SHA-1 but with 64 rounds and 8 state vars).

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn sha256_hex(data: &[u8]) -> String {
    sha256(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// Truncate SHA-256 to u64 for comparison benchmarks.
pub fn sha256_u32(data: &[u8]) -> u32 {
    let hash = sha256(data);
    u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]])
}

// =============================================================================
// CRC32 — Not a hash, but often confused with one
// =============================================================================
// Cyclic Redundancy Check. Detects accidental corruption (bit flips in transit).
// NOT suitable for hash tables (poor distribution) or security (trivially reversible).
// Used in: Ethernet frames, ZIP files, PNG, gzip.
//
// Included here to explain WHY it's different from a hash function.

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB88320; // standard polynomial
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
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
    // =========================================================================
    // Part 1: Non-cryptographic hash comparison
    // =========================================================================
    println!("=== Non-Cryptographic Hash Functions ===");
    println!("  (fast, for hash tables, sharding, bloom filters)\n");

    let test_inputs = [
        "hello",
        "hello!",
        "Hello",
        "consistent-hashing",
        "user:12345",
    ];

    println!(
        "  {:<22} {:>12} {:>12} {:>12} {:>12}",
        "Input", "FNV-1a", "DJB2", "Murmur3", "xxHash32"
    );
    println!("  {}", "-".repeat(72));

    for input in &test_inputs {
        let bytes = input.as_bytes();
        println!(
            "  {:<22} {:>12} {:>12} {:>12} {:>12}",
            input,
            format!("0x{:08x}", fnv1a_32(bytes)),
            format!("0x{:08x}", djb2(bytes)),
            format!("0x{:08x}", murmur3_32(bytes, 0)),
            format!("0x{:08x}", xxhash32(bytes, 0)),
        );
    }

    // =========================================================================
    // Part 2: Cryptographic hash comparison
    // =========================================================================
    println!("\n=== Cryptographic Hash Functions ===");
    println!("  (slower, for security, integrity, signatures)\n");

    let crypto_inputs = ["hello", "hello!", "Hello"];

    for input in &crypto_inputs {
        let bytes = input.as_bytes();
        println!("  Input: \"{input}\"");
        println!("    MD5:    {}", md5_hex(bytes));
        println!("    SHA-1:  {}", sha1_hex(bytes));
        println!("    SHA-256: {}", sha256_hex(bytes));
        println!();
    }

    // =========================================================================
    // Part 3: CRC32 (not a hash!)
    // =========================================================================
    println!("=== CRC32 (error detection, NOT a hash) ===\n");
    println!(
        "  crc32(\"hello\") = 0x{:08x}",
        crc32(b"hello")
    );
    println!(
        "  crc32(\"hello!\") = 0x{:08x}",
        crc32(b"hello!")
    );

    // =========================================================================
    // Part 4: Avalanche test — all functions
    // =========================================================================
    println!("\n=== Avalanche Effect (ideal = 50%) ===");
    println!("  Flip 1 input bit → what % of output bits change?\n");
    let base = b"test-input-for-avalanche";
    println!("  FNV-1a:    {:.1}%", avalanche_test(fnv1a_32, base));
    println!("  DJB2:      {:.1}%", avalanche_test(djb2, base));
    println!("  Murmur3:   {:.1}%", avalanche_test(|d| murmur3_32(d, 0), base));
    println!("  xxHash32:  {:.1}%", avalanche_test(|d| xxhash32(d, 0), base));
    println!("  CRC32:     {:.1}%", avalanche_test(crc32, base));
    println!("  SHA-256*:  {:.1}%", avalanche_test(sha256_u32, base));

    // =========================================================================
    // Part 5: Distribution test — all functions
    // =========================================================================
    println!("\n=== Distribution Quality (chi-sq/buckets, lower = better, <1.5 is good) ===\n");
    let n = 100_000;
    let b = 1000;
    println!("  FNV-1a:    {:.4}", distribution_test(fnv1a_32, n, b));
    println!("  DJB2:      {:.4}", distribution_test(djb2, n, b));
    println!("  Murmur3:   {:.4}", distribution_test(|d| murmur3_32(d, 0), n, b));
    println!("  xxHash32:  {:.4}", distribution_test(|d| xxhash32(d, 0), n, b));
    println!("  CRC32:     {:.4}", distribution_test(crc32, n, b));
    println!("  SHA-256*:  {:.4}", distribution_test(sha256_u32, n, b));

    // =========================================================================
    // Part 6: Speed comparison
    // =========================================================================
    println!("\n=== Speed Comparison (time to hash 100K keys) ===\n");

    let keys: Vec<String> = (0..100_000).map(|i| format!("key-{i}")).collect();

    let t = std::time::Instant::now();
    for k in &keys {
        std::hint::black_box(fnv1a_32(k.as_bytes()));
    }
    let fnv_time = t.elapsed();

    let t = std::time::Instant::now();
    for k in &keys {
        std::hint::black_box(murmur3_32(k.as_bytes(), 0));
    }
    let murmur_time = t.elapsed();

    let t = std::time::Instant::now();
    for k in &keys {
        std::hint::black_box(xxhash32(k.as_bytes(), 0));
    }
    let xx_time = t.elapsed();

    let t = std::time::Instant::now();
    for k in &keys {
        std::hint::black_box(md5_hex(k.as_bytes()));
    }
    let md5_time = t.elapsed();

    let t = std::time::Instant::now();
    for k in &keys {
        std::hint::black_box(sha1(k.as_bytes()));
    }
    let sha1_time = t.elapsed();

    let t = std::time::Instant::now();
    for k in &keys {
        std::hint::black_box(sha256(k.as_bytes()));
    }
    let sha256_time = t.elapsed();

    println!("  FNV-1a:    {:>8.2}ms  (non-crypto)", fnv_time.as_secs_f64() * 1000.0);
    println!("  Murmur3:   {:>8.2}ms  (non-crypto)", murmur_time.as_secs_f64() * 1000.0);
    println!("  xxHash32:  {:>8.2}ms  (non-crypto)", xx_time.as_secs_f64() * 1000.0);
    println!("  MD5:       {:>8.2}ms  (crypto, BROKEN)", md5_time.as_secs_f64() * 1000.0);
    println!("  SHA-1:     {:>8.2}ms  (crypto, BROKEN)", sha1_time.as_secs_f64() * 1000.0);
    println!("  SHA-256:   {:>8.2}ms  (crypto, secure)", sha256_time.as_secs_f64() * 1000.0);

    // =========================================================================
    // Part 7: Cheat sheet
    // =========================================================================
    println!("\n=== Which Hash to Use? ===\n");
    println!("  ┌───────────────────┬──────────┬────────────┬──────────────────────────────┐");
    println!("  │ Function          │ Bits     │ Status     │ Use For                      │");
    println!("  ├───────────────────┼──────────┼────────────┼──────────────────────────────┤");
    println!("  │ FNV-1a            │ 32/64    │ ✓ OK       │ Hash tables, quick hashing   │");
    println!("  │ DJB2              │ 32       │ ✓ OK       │ Simple hash tables           │");
    println!("  │ MurmurHash3       │ 32/128   │ ✓ OK       │ Bloom filters, partitioning  │");
    println!("  │ xxHash            │ 32/64    │ ✓ OK       │ Checksums, fast hashing      │");
    println!("  │ CRC32             │ 32       │ ✓ OK       │ Error detection ONLY         │");
    println!("  ├───────────────────┼──────────┼────────────┼──────────────────────────────┤");
    println!("  │ MD5               │ 128      │ ✗ BROKEN   │ Checksums (non-security)     │");
    println!("  │ SHA-1             │ 160      │ ✗ BROKEN   │ Git (legacy), avoid new use  │");
    println!("  │ SHA-256           │ 256      │ ✓ SECURE   │ TLS, signing, blockchain     │");
    println!("  │ SHA-3 / BLAKE3    │ 256+     │ ✓ SECURE   │ Modern alternative to SHA-2  │");
    println!("  │ bcrypt/argon2     │ varies   │ ✓ SECURE   │ Password storage ONLY        │");
    println!("  └───────────────────┴──────────┴────────────┴──────────────────────────────┘");
}
