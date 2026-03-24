use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

// =============================================================================
// Cache-Aside (Lazy Loading) — the most common caching pattern
//
//   Client → App → Cache? ──hit──→ return cached value
//                    │
//                   miss
//                    │
//                    ▼
//                   DB ──→ store in cache ──→ return value
//
//   App is responsible for all cache logic:
//     read:  check cache → miss → query DB → populate cache
//     write: update DB → invalidate cache (or update cache)
//
//   Pros: only caches what's actually requested
//   Cons: first request always misses (cold cache), stale data possible
// =============================================================================

// Simulate a slow database
struct FakeDb {
    data: HashMap<String, String>,
    query_count: usize,
}

impl FakeDb {
    fn new() -> Self {
        let mut data = HashMap::new();
        data.insert("user:1".into(), r#"{"name":"Alice","email":"alice@example.com"}"#.into());
        data.insert("user:2".into(), r#"{"name":"Bob","email":"bob@example.com"}"#.into());
        data.insert("user:3".into(), r#"{"name":"Charlie","email":"charlie@example.com"}"#.into());
        Self { data, query_count: 0 }
    }

    fn get(&mut self, key: &str) -> Option<String> {
        self.query_count += 1;
        thread::sleep(Duration::from_millis(50)); // simulate DB latency
        self.data.get(key).cloned()
    }
}

pub fn demo() {
    println!("\n  ═══ Cache-Aside (Lazy Loading) ═══\n");
    println!("    Using moka: production-grade concurrent cache (like Java Caffeine).\n");

    let cache: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(5))
        .build();

    let db = Arc::new(Mutex::new(FakeDb::new()));

    // Cache-aside pattern: check cache first, then DB
    let get_user = |key: &str| -> (String, bool) {
        // Step 1: Check cache
        if let Some(val) = cache.get(&key.to_string()) {
            return (val, true); // cache hit
        }
        // Step 2: Cache miss → query DB
        let val = db.lock().unwrap().get(key).unwrap_or_default();
        // Step 3: Populate cache
        cache.insert(key.to_string(), val.clone());
        (val, false) // cache miss
    };

    // First access: always misses (cold cache)
    println!("    First access (cold cache):\n");
    for key in &["user:1", "user:2", "user:1"] {
        let start = Instant::now();
        let (val, hit) = get_user(key);
        println!("    GET {} → {} [{:?}] {}",
            key, &val[..30.min(val.len())], start.elapsed(),
            if hit { "← CACHE HIT" } else { "← cache miss (hit DB)" });
    }

    // Second access: cache hit (fast!)
    println!("\n    Second access (warm cache):\n");
    for key in &["user:1", "user:2", "user:3"] {
        let start = Instant::now();
        let (val, hit) = get_user(key);
        println!("    GET {} → {} [{:?}] {}",
            key, &val[..30.min(val.len())], start.elapsed(),
            if hit { "← CACHE HIT" } else { "← cache miss (hit DB)" });
    }

    let db_queries = db.lock().unwrap().query_count;
    println!("\n    Total DB queries: {} (cache saved the rest)", db_queries);
    println!("    Cache entries: {}\n", cache.entry_count());
}
