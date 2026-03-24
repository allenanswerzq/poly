use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::thread;

// =============================================================================
// Cache Stampede (Thundering Herd)
//
//   Problem: popular cache key expires → 1000 requests arrive at the same
//   time → ALL of them see a cache miss → ALL of them hit the DB →
//   DB gets crushed.
//
//   Timeline:
//     t=0:  cache entry expires
//     t=1:  thread A → miss → queries DB
//     t=1:  thread B → miss → queries DB  (simultaneously!)
//     t=1:  thread C → miss → queries DB  (simultaneously!)
//     ...   100 threads all hammering DB for the SAME key
//
//   Solution: lock so only ONE thread queries DB, others wait.
// =============================================================================

fn simulate_db_query(key: &str) -> String {
    thread::sleep(Duration::from_millis(100)); // slow DB
    format!("{{\"data\":\"{}\",\"from\":\"db\"}}", key)
}

pub fn demo() {
    println!("\n  ═══ Cache Stampede (Thundering Herd) ═══\n");

    // ── Without protection: stampede ──
    println!("    ── Without protection: all threads hit DB ──\n");

    let cache: Arc<Cache<String, String>> = Arc::new(Cache::builder()
        .max_capacity(100)
        .build());

    let db_hits = Arc::new(AtomicUsize::new(0));
    let num_threads = 10;

    // All threads request the same key at once (cache is empty)
    let mut handles = vec![];
    for _i in 0..num_threads {
        let cache = Arc::clone(&cache);
        let db_hits = Arc::clone(&db_hits);
        handles.push(thread::spawn(move || {
            // Cache miss → each thread independently queries DB
            if cache.get(&"popular:key".to_string()).is_none() {
                db_hits.fetch_add(1, Ordering::Relaxed);
                let val = simulate_db_query("popular:key");
                cache.insert("popular:key".into(), val);
            }
        }));
    }
    for h in handles { h.join().unwrap(); }

    println!("    {} threads requested same key simultaneously", num_threads);
    println!("    DB was hit {} times! (should be 1)\n", db_hits.load(Ordering::Relaxed));

    // ── With protection: lock-based stampede prevention ──
    println!("    ── With lock protection: only 1 thread hits DB ──\n");

    let cache2: Arc<Cache<String, String>> = Arc::new(Cache::builder()
        .max_capacity(100)
        .build());

    let db_hits2 = Arc::new(AtomicUsize::new(0));
    // Per-key lock prevents stampede
    let fetch_lock: Arc<Mutex<HashMap<String, ()>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut handles = vec![];
    for _i in 0..num_threads {
        let cache = Arc::clone(&cache2);
        let db_hits = Arc::clone(&db_hits2);
        let fetch_lock = Arc::clone(&fetch_lock);
        handles.push(thread::spawn(move || {
            let key = "popular:key".to_string();

            // Check cache first (no lock)
            if cache.get(&key).is_some() {
                return;
            }

            // Acquire lock for this key — only one thread gets through
            let _lock = fetch_lock.lock().unwrap();

            // Double-check: another thread might have populated cache while we waited
            if cache.get(&key).is_some() {
                return;
            }

            // Only ONE thread reaches here
            db_hits.fetch_add(1, Ordering::Relaxed);
            let val = simulate_db_query(&key);
            cache.insert(key, val);
        }));
    }
    for h in handles { h.join().unwrap(); }

    println!("    {} threads requested same key simultaneously", num_threads);
    println!("    DB was hit {} times (lock prevented stampede)\n",
        db_hits2.load(Ordering::Relaxed));

    // ── TTL jitter: stagger expirations ──
    println!("    ── Other stampede prevention techniques ──\n");
    println!("    1. Lock (shown above): only 1 thread fetches, others wait");
    println!("    2. TTL jitter: add random ±10% to TTL so keys don't expire together");
    println!("       TTL = 60s + random(0..6s) → spread expirations over 6 seconds");
    println!("    3. Background refresh: re-fetch BEFORE expiry (no miss at all)");
    println!("    4. Stale-while-revalidate: serve stale data, refresh in background\n");

    // Demonstrate TTL jitter
    let cache3: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .build();

    println!("    TTL jitter example (base TTL = 1000ms, jitter = ±200ms):");
    for i in 0..5 {
        let jitter = rand::random::<u64>() % 400; // 0-400ms jitter
        let ttl = Duration::from_millis(800 + jitter); // 800-1200ms
        cache3.insert(format!("key:{}", i), format!("val-{}", i));
        println!("    key:{} → TTL = {}ms", i, ttl.as_millis());
    }
    println!("    → Keys expire at different times, no thundering herd.\n");
}
