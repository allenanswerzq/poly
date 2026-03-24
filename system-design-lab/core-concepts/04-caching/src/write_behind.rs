use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

// =============================================================================
// Write-Behind (Write-Back) — write to cache, async flush to DB later
//
//   Client → App → write to Cache → return success IMMEDIATELY
//                          │
//                          └──→ async background thread flushes to DB
//
//   Fastest writes: client doesn't wait for DB.
//   But risky: if the app crashes before flush, data is LOST.
//
//   Good for: analytics counters, non-critical metrics, view counts
//   Bad for: banking, payments (data loss = unacceptable)
// =============================================================================

struct WriteBehindCache {
    cache: Cache<String, String>,
    // "Dirty" entries waiting to be flushed to DB
    write_buffer: Arc<Mutex<Vec<(String, String)>>>,
    db: Arc<Mutex<HashMap<String, String>>>,
}

impl WriteBehindCache {
    fn new() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(100)
                .build(),
            write_buffer: Arc::new(Mutex::new(Vec::new())),
            db: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Write-behind: write to cache + buffer, return immediately
    fn set(&self, key: &str, value: &str) {
        // Step 1: Write to cache (instant)
        self.cache.insert(key.to_string(), value.to_string());

        // Step 2: Add to write buffer (will be flushed later)
        self.write_buffer.lock().unwrap().push((key.to_string(), value.to_string()));

        // Client returns here — doesn't wait for DB write!
    }

    fn get(&self, key: &str) -> Option<String> {
        self.cache.get(&key.to_string())
    }

    // Background flush — runs periodically (like every 1 second)
    fn flush(&self) -> usize {
        let entries: Vec<(String, String)> = {
            let mut buf = self.write_buffer.lock().unwrap();
            buf.drain(..).collect()
        };
        let count = entries.len();
        if count > 0 {
            thread::sleep(Duration::from_millis(20)); // simulate batch DB write
            let mut db = self.db.lock().unwrap();
            for (k, v) in entries {
                db.insert(k, v);
            }
        }
        count
    }

    fn db_count(&self) -> usize {
        self.db.lock().unwrap().len()
    }

    fn buffer_count(&self) -> usize {
        self.write_buffer.lock().unwrap().len()
    }
}

pub fn demo() {
    println!("\n  ═══ Write-Behind (Write-Back) ═══\n");

    let store = WriteBehindCache::new();

    // Fast writes — all go to cache + buffer
    println!("    Writing 5 entries (cache only, DB write deferred):\n");
    let start = std::time::Instant::now();
    for i in 1..=5 {
        store.set(&format!("counter:{}", i), &format!("{}", i * 100));
    }
    println!("    5 writes completed in {:?} (no DB wait!)", start.elapsed());
    println!("    Cache entries: {}", store.cache.entry_count());
    println!("    Write buffer:  {} entries pending", store.buffer_count());
    println!("    DB entries:    {} (nothing yet!)\n", store.db_count());

    // Read from cache (instant, even though DB hasn't been written yet)
    println!("    GET counter:3 → {:?} (from cache, DB is empty!)\n",
        store.get("counter:3"));

    // Background flush — batch write all pending entries to DB
    println!("    Flushing write buffer to DB...\n");
    let flushed = store.flush();
    println!("    Flushed {} entries to DB", flushed);
    println!("    Write buffer:  {} entries pending", store.buffer_count());
    println!("    DB entries:    {} (now persisted!)\n", store.db_count());

    // Simulate the risk: data loss before flush
    println!("    ⚠ Risk: if app crashes BEFORE flush, buffered writes are LOST.");
    store.set("important:data", "this could be lost");
    println!("    SET important:data (in buffer, not yet in DB)");
    println!("    Buffer: {} pending, DB: {} persisted", store.buffer_count(), store.db_count());
    println!("    → If we crash now, 'important:data' is gone forever.\n");

    println!("    Write-behind: fastest writes, but risk of data loss.");
    println!("    Best for: view counts, analytics, non-critical counters.\n");
}
