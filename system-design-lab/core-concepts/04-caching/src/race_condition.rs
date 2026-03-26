use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

// =============================================================================
// Cache-Aside Race Condition — the subtle bug
//
// Thread A: cache miss → starts DB query (slow)
// Thread B: updates DB → deletes cache key
// Thread A: finishes DB query with OLD data → writes OLD data to cache
// Result: cache has stale data until TTL expires
//
// This demo shows the race happening and the lease-based fix.
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Cache-Aside Race Condition ═══\n");

    let cache: Arc<Cache<String, String>> = Arc::new(Cache::builder()
        .max_capacity(100)
        .build());
    let db = Arc::new(Mutex::new(HashMap::from([
        ("user:42".to_string(), "Alice".to_string()),
    ])));

    // Show the race condition
    println!("    Demonstrating the race condition:\n");

    let cache_clone = Arc::clone(&cache);
    let db_clone = Arc::clone(&db);

    // Thread A: slow reader (simulates a cache miss + slow DB query)
    let thread_a = thread::spawn({
        let cache = Arc::clone(&cache_clone);
        let db = Arc::clone(&db_clone);
        move || {
            // Step 1: cache miss
            println!("    t=0   Thread A: cache miss for user:42");

            // Step 2: start slow DB query (reads OLD value)
            println!("    t=1   Thread A: SELECT * FROM users WHERE id=42 (slow query started)");
            thread::sleep(Duration::from_millis(100)); // slow DB query

            // Step 3: Thread A gets the OLD value (DB was "Alice" when query started)
            // But by now, Thread B has already updated it to "Bob"!
            let old_value = "Alice".to_string(); // this is what the slow query returned
            println!("    t=100 Thread A: DB returned 'Alice' (from BEFORE Thread B's update)");

            // Step 4: Thread A writes stale data to cache
            cache.insert("user:42".to_string(), old_value);
            println!("    t=101 Thread A: SET cache user:42 = 'Alice' ← STALE!");
        }
    });

    // Thread B: writer (updates DB + invalidates cache while Thread A is still querying)
    let thread_b = thread::spawn({
        let cache = Arc::clone(&cache_clone);
        let db = Arc::clone(&db_clone);
        move || {
            thread::sleep(Duration::from_millis(50)); // start after Thread A's query begins

            // Update DB
            db.lock().unwrap().insert("user:42".to_string(), "Bob".to_string());
            println!("    t=50  Thread B: UPDATE users SET name='Bob' WHERE id=42");

            // Invalidate cache
            cache.invalidate(&"user:42".to_string());
            println!("    t=51  Thread B: DEL cache user:42 (invalidated)");
        }
    });

    thread_a.join().unwrap();
    thread_b.join().unwrap();

    // Show the result
    let cached = cache.get(&"user:42".to_string());
    let db_val = db.lock().unwrap().get("user:42").cloned();
    println!("\n    Result:");
    println!("      Cache says: {:?}", cached);
    println!("      DB says:    {:?}", db_val);
    println!("      Match: {}  ← STALE if 'Alice' in cache, 'Bob' in DB",
        cached.as_deref() == db_val.as_deref());

    // ── Fix: version-based keys ──
    println!("\n    Fix: version-based cache keys\n");

    let version: Arc<Mutex<u32>> = Arc::new(Mutex::new(1));
    let cache2: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .build();

    // Initial state
    let v = *version.lock().unwrap();
    let key = format!("user:42:v{}", v);
    cache2.insert(key.clone(), "Alice".to_string());
    println!("    Initial: cache key='{}' → 'Alice'", key);

    // Writer bumps version
    {
        let mut v = version.lock().unwrap();
        *v += 1; // version 1 → 2
    }
    let new_v = *version.lock().unwrap();
    let new_key = format!("user:42:v{}", new_v);
    cache2.insert(new_key.clone(), "Bob".to_string());
    println!("    Writer: bumps version to {}, new key='{}'", new_v, new_key);

    // Old Thread A tries to write with old version key — it's now irrelevant
    let old_key = format!("user:42:v{}", new_v - 1);
    println!("    Thread A writes to old key '{}' → nobody reads it anymore", old_key);

    // Reader uses current version
    let current_v = *version.lock().unwrap();
    let read_key = format!("user:42:v{}", current_v);
    let val = cache2.get(&read_key).unwrap_or_default();
    println!("    Reader: reads '{}' → '{}' ← correct!\n", read_key, val);
}
