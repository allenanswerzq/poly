use moka::sync::Cache;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// =============================================================================
// Cache Warming — pre-populate cache before taking traffic
//
// Problem: after deploy/restart, cache is empty (cold).
//          100% miss rate → DB crushed → outage.
// Solution: load hot keys into cache BEFORE switching traffic.
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Cache Warming ═══\n");

    // Simulate a DB with popular data
    let db: HashMap<String, String> = (0..100)
        .map(|i| (format!("product:{}", i), format!("{{\"name\":\"Product {}\",\"price\":{}}}", i, i * 10 + 99)))
        .collect();

    // Hot keys: the top 20 most-requested products (known from analytics)
    let hot_keys: Vec<String> = (0..20).map(|i| format!("product:{}", i)).collect();

    // ── Without warming: cold cache ──
    println!("    Without warming (cold cache):\n");
    let cold_cache: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(60))
        .build();

    let mut hits = 0;
    let mut misses = 0;
    // Simulate 100 requests (biased toward hot keys)
    for i in 0..100 {
        let key = format!("product:{}", i % 30); // first 30 products are popular
        if cold_cache.get(&key).is_some() {
            hits += 1;
        } else {
            misses += 1;
            // Fetch from DB and populate
            if let Some(val) = db.get(&key) {
                cold_cache.insert(key, val.clone());
            }
        }
    }
    println!("    100 requests: {} hits, {} misses ({:.0}% hit rate)",
        hits, misses, hits as f64 / (hits + misses) as f64 * 100.0);
    println!("    → {} DB queries in the first burst!\n", misses);

    // ── With warming: pre-populate hot keys ──
    println!("    With warming (pre-load hot keys before traffic):\n");
    let warm_cache: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(60))
        .build();

    // Warming step: load hot keys from DB BEFORE traffic starts
    let start = Instant::now();
    for key in &hot_keys {
        if let Some(val) = db.get(key) {
            warm_cache.insert(key.clone(), val.clone());
        }
    }
    println!("    Pre-warmed {} hot keys in {:?}", hot_keys.len(), start.elapsed());

    let mut hits = 0;
    let mut misses = 0;
    for i in 0..100 {
        let key = format!("product:{}", i % 30);
        if warm_cache.get(&key).is_some() {
            hits += 1;
        } else {
            misses += 1;
            if let Some(val) = db.get(&key) {
                warm_cache.insert(key, val.clone());
            }
        }
    }
    println!("    100 requests: {} hits, {} misses ({:.0}% hit rate)",
        hits, misses, hits as f64 / (hits + misses) as f64 * 100.0);
    println!("    → only {} DB queries (the non-hot keys)\n", misses);

    println!("    Warming strategies:");
    println!("    1. Script: load TOP_N keys from DB before deploy");
    println!("    2. Replication: sync old Redis → new Redis before cutover");
    println!("    3. Traffic replay: replay recorded reads against new cache");
    println!("    4. Gradual rollout: send 1%→10%→100% traffic slowly\n");
}
