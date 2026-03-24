use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// =============================================================================
// Write-Through — write to cache AND DB synchronously on every write
//
//   Client → App → write to Cache ──→ write to DB ──→ return success
//
//   Cache is ALWAYS consistent with DB (no stale data).
//   But writes are slower: must wait for both cache + DB.
//
//   Good for: data that's read frequently right after writing
//   Bad for: write-heavy workloads (double write latency)
// =============================================================================

struct WriteThroughCache {
    cache: Cache<String, String>,
    db: Arc<Mutex<HashMap<String, String>>>,
}

impl WriteThroughCache {
    fn new() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(100)
                .time_to_live(Duration::from_secs(30))
                .build(),
            db: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Write-through: write to BOTH cache and DB
    fn set(&self, key: &str, value: &str) {
        // Step 1: Write to cache
        self.cache.insert(key.to_string(), value.to_string());

        // Step 2: Write to DB (synchronously — must succeed before returning)
        std::thread::sleep(Duration::from_millis(10)); // simulate DB write
        self.db.lock().unwrap().insert(key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> Option<String> {
        // Always read from cache (it's always up-to-date with write-through)
        self.cache.get(&key.to_string())
    }

    fn get_from_db(&self, key: &str) -> Option<String> {
        self.db.lock().unwrap().get(key).cloned()
    }
}

pub fn demo() {
    println!("\n  ═══ Write-Through ═══\n");

    let store = WriteThroughCache::new();

    // Write — updates both cache and DB simultaneously
    println!("    SET user:1 (writes to cache + DB synchronously)\n");
    store.set("user:1", r#"{"name":"Alice","score":100}"#);
    store.set("user:2", r#"{"name":"Bob","score":200}"#);

    // Read — always from cache (guaranteed fresh)
    println!("    GET from cache: {:?}", store.get("user:1"));
    println!("    GET from DB:    {:?}", store.get_from_db("user:1"));
    println!("    → Both are identical (write-through keeps them in sync)\n");

    // Update — cache and DB both updated atomically
    println!("    UPDATE user:1 score to 150\n");
    store.set("user:1", r#"{"name":"Alice","score":150}"#);
    println!("    Cache: {:?}", store.get("user:1"));
    println!("    DB:    {:?}", store.get_from_db("user:1"));
    println!("    → Both updated together. No stale data.\n");

    println!("    Write-through: cache always consistent, but slower writes.");
    println!("    Best for: user profiles, settings (read often after write).\n");
}
