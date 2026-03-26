use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// =============================================================================
// Read-Through — the cache fetches from DB automatically on miss.
// Your app never touches the DB directly. Cache is the only data source.
//
//   Cache-aside:   app → cache.get() → miss → APP queries DB → APP populates cache
//   Read-through:  app → cache.get() → miss → CACHE queries DB → CACHE populates itself
// =============================================================================

// Simulate a read-through cache: wraps a DB and automatically loads on miss
struct ReadThroughCache {
    cache: Cache<String, String>,
    db: Arc<Mutex<HashMap<String, String>>>,
    db_query_count: Arc<Mutex<usize>>,
}

impl ReadThroughCache {
    fn new() -> Self {
        let mut db_data = HashMap::new();
        db_data.insert("user:1".into(), r#"{"name":"Alice"}"#.into());
        db_data.insert("user:2".into(), r#"{"name":"Bob"}"#.into());
        db_data.insert("user:3".into(), r#"{"name":"Charlie"}"#.into());

        Self {
            cache: Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(5))
                .build(),
            db: Arc::new(Mutex::new(db_data)),
            db_query_count: Arc::new(Mutex::new(0)),
        }
    }

    // The read-through magic: app just calls get(), cache handles everything
    fn get(&self, key: &str) -> Option<String> {
        // Check cache first
        if let Some(val) = self.cache.get(&key.to_string()) {
            return Some(val);
        }

        // Cache miss → cache itself fetches from DB (not the app!)
        let db = self.db.lock().unwrap();
        *self.db_query_count.lock().unwrap() += 1;
        std::thread::sleep(Duration::from_millis(50)); // simulate DB latency

        if let Some(val) = db.get(key) {
            self.cache.insert(key.to_string(), val.clone());
            Some(val.clone())
        } else {
            None
        }
    }
}

pub fn demo() {
    println!("\n  ═══ Read-Through Cache ═══\n");
    println!("    App never touches DB. Cache auto-fetches on miss.\n");

    let cache = ReadThroughCache::new();

    // First call: miss → cache fetches from DB
    let start = std::time::Instant::now();
    let val = cache.get("user:1").unwrap();
    println!("    cache.get(\"user:1\") → {} [{:?}] (miss → auto DB fetch)",
        val, start.elapsed());

    // Second call: hit → no DB query
    let start = std::time::Instant::now();
    let val = cache.get("user:1").unwrap();
    println!("    cache.get(\"user:1\") → {} [{:?}] (hit → from cache)",
        val, start.elapsed());

    // Another key: miss → auto fetch
    let _ = cache.get("user:2");
    let _ = cache.get("user:3");
    let _ = cache.get("user:2"); // hit

    let queries = *cache.db_query_count.lock().unwrap();
    println!("\n    Total DB queries: {} (cache handled misses automatically)", queries);
    println!("    App code just calls cache.get() — doesn't know about DB.\n");
}
