use moka::sync::Cache;
use std::collections::HashSet;
use std::time::Duration;

// =============================================================================
// Cache Penetration — queries for data that doesn't exist
//
//   Problem:
//     GET user:999999 → cache miss → DB query → not found → not cached
//     GET user:999999 → cache miss → DB query → not found → not cached
//     ... every request hits DB! Attacker can exploit this.
//
//   Solutions:
//     1. Cache negative results (NULL) with short TTL
//     2. Bloom filter: check if key could exist before querying DB
// =============================================================================

// Simulate a DB that only has users 1-100
fn db_lookup(key: &str) -> Option<String> {
    // Parse "user:N" and only return data for ids 1-100
    if let Some(id_str) = key.strip_prefix("user:") {
        if let Ok(id) = id_str.parse::<u64>() {
            if id >= 1 && id <= 100 {
                return Some(format!("{{\"id\":{},\"name\":\"User{}\"}}", id, id));
            }
        }
    }
    None // doesn't exist
}

// Simple bloom filter (hash set for clarity — real bloom filter uses bit array)
struct SimpleBloomFilter {
    // In production: use a bit array with multiple hash functions
    // Here we use a HashSet for clarity of concept
    known_keys: HashSet<String>,
}

impl SimpleBloomFilter {
    fn new() -> Self {
        let mut known_keys = HashSet::new();
        // Pre-populate with known valid key patterns
        for i in 1..=100 {
            known_keys.insert(format!("user:{}", i));
        }
        Self { known_keys }
    }

    fn might_exist(&self, key: &str) -> bool {
        // Real bloom filter: might return true for non-existent keys (false positive)
        // but NEVER returns false for existing keys (no false negatives)
        self.known_keys.contains(key)
    }
}

pub fn demo() {
    println!("\n  ═══ Cache Penetration ═══\n");

    // ── Without protection: every missing key hits DB ──
    println!("    ── Without protection ──\n");

    let cache: Cache<String, Option<String>> = Cache::builder()
        .max_capacity(1000)
        .build();

    let mut db_hits = 0;
    let test_keys = ["user:1", "user:999", "user:999", "user:888", "user:888", "user:1"];

    for key in &test_keys {
        if cache.get(&key.to_string()).is_some() {
            println!("    GET {} → cache hit", key);
            continue;
        }
        db_hits += 1;
        let result = db_lookup(key);
        // BUG: we DON'T cache None results → non-existent keys always hit DB
        if let Some(ref val) = result {
            cache.insert(key.to_string(), Some(val.clone()));
        }
        println!("    GET {} → DB query → {:?}",
            key, result.as_deref().unwrap_or("NOT FOUND"));
    }
    println!("\n    DB hits: {} (non-existent keys hit DB every time!)\n", db_hits);

    // ── Solution 1: Cache negative results ──
    println!("    ── Solution 1: Cache negative results (cache NULL) ──\n");

    let cache2: Cache<String, Option<String>> = Cache::builder()
        .max_capacity(1000)
        .time_to_live(Duration::from_secs(60)) // short TTL for negatives
        .build();

    let mut db_hits2 = 0;
    for key in &test_keys {
        if let Some(cached) = cache2.get(&key.to_string()) {
            let display = cached.as_deref().unwrap_or("NULL (negative cache)");
            println!("    GET {} → cache hit: {}", key, display);
            continue;
        }
        db_hits2 += 1;
        let result = db_lookup(key);
        // Cache BOTH hits AND misses (None = negative cache)
        cache2.insert(key.to_string(), result.clone());
        println!("    GET {} → DB query → {:?}",
            key, result.as_deref().unwrap_or("NOT FOUND (now cached as NULL)"));
    }
    println!("\n    DB hits: {} (negative caching prevents repeated lookups)\n", db_hits2);

    // ── Solution 2: Bloom filter ──
    println!("    ── Solution 2: Bloom filter (pre-check before DB) ──\n");

    let bloom = SimpleBloomFilter::new();
    let mut db_hits3 = 0;

    let test_keys2 = ["user:1", "user:999", "user:50", "user:888", "user:100"];
    for key in &test_keys2 {
        if !bloom.might_exist(key) {
            println!("    GET {} → bloom filter says NO → skip DB entirely", key);
            continue;
        }
        db_hits3 += 1;
        let result = db_lookup(key);
        println!("    GET {} → bloom says MAYBE → DB query → {:?}",
            key, result.as_deref().unwrap_or("not found"));
    }
    println!("\n    DB hits: {} (bloom filter rejected non-existent keys)", db_hits3);

    println!("\n    Cache penetration solutions:");
    println!("    1. Cache NULL: simple, but uses memory for non-existent keys");
    println!("    2. Bloom filter: O(1) pre-check, tiny memory (~1 byte per key)");
    println!("    In production: Redis + bloom filter module, or app-level bloom.\n");
}
