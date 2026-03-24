use moka::sync::Cache;
use std::time::{Duration, Instant};
use std::thread;

// =============================================================================
// TTL + LRU Eviction — what happens when cache is full or entries expire
//
//   TTL (Time-To-Live):
//     Entry inserted at t=0 with TTL=5s
//     GET at t=3s → hit (valid)
//     GET at t=6s → miss (expired, evicted)
//
//   LRU (Least Recently Used):
//     Cache capacity = 3
//     PUT a, PUT b, PUT c → [a, b, c]
//     PUT d → evict 'a' (least recently used) → [b, c, d]
//     GET b → 'b' moves to front → [c, d, b]
//     PUT e → evict 'c' (LRU) → [d, b, e]
//
//   moka uses a TinyLFU-based eviction (better than pure LRU).
// =============================================================================

pub fn demo() {
    println!("\n  ═══ TTL + LRU Eviction ═══\n");

    // ── TTL Demo ──
    println!("    ── TTL (Time-To-Live) ──\n");

    let cache: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_millis(500)) // expire after 500ms
        .build();

    cache.insert("session:abc".into(), "user-42".into());
    println!("    SET session:abc (TTL = 500ms)");

    println!("    GET at t=0ms:   {:?}", cache.get(&"session:abc".to_string()));
    thread::sleep(Duration::from_millis(300));
    println!("    GET at t=300ms: {:?}", cache.get(&"session:abc".to_string()));
    thread::sleep(Duration::from_millis(300));

    // Force eviction of expired entries
    cache.run_pending_tasks();

    println!("    GET at t=600ms: {:?}  ← expired!", cache.get(&"session:abc".to_string()));

    // ── LRU Eviction Demo ──
    println!("\n    ── LRU Eviction (capacity = 3) ──\n");

    let small_cache: Cache<String, String> = Cache::builder()
        .max_capacity(3)
        .build();

    // Fill cache to capacity
    small_cache.insert("a".into(), "val-a".into());
    small_cache.insert("b".into(), "val-b".into());
    small_cache.insert("c".into(), "val-c".into());
    small_cache.run_pending_tasks();
    println!("    PUT a, b, c → cache full ({} entries)", small_cache.entry_count());

    // Insert 4th entry → evicts least recently used
    small_cache.insert("d".into(), "val-d".into());
    small_cache.run_pending_tasks();
    println!("    PUT d → eviction triggered ({} entries)", small_cache.entry_count());
    println!("    GET a: {:?}  ← evicted (least recently used)", small_cache.get(&"a".to_string()));
    println!("    GET b: {:?}", small_cache.get(&"b".to_string()));
    println!("    GET d: {:?}", small_cache.get(&"d".to_string()));

    // Access 'b' to make it recently used, then insert more
    let _ = small_cache.get(&"b".to_string()); // 'b' is now most recently used
    small_cache.insert("e".into(), "val-e".into());
    small_cache.run_pending_tasks();
    println!("\n    GET b (touch it), then PUT e:");
    println!("    GET b: {:?}  ← survived (recently used)", small_cache.get(&"b".to_string()));
    println!("    GET c: {:?}  ← evicted (wasn't used recently)", small_cache.get(&"c".to_string()));
    println!("    GET e: {:?}", small_cache.get(&"e".to_string()));

    // ── Time-To-Idle Demo ──
    println!("\n    ── Time-To-Idle (expire if not accessed) ──\n");

    let idle_cache: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .time_to_idle(Duration::from_millis(400)) // expire if idle for 400ms
        .build();

    idle_cache.insert("hot-key".into(), "frequently accessed".into());
    idle_cache.insert("cold-key".into(), "rarely accessed".into());

    // Keep accessing hot-key, ignore cold-key
    for i in 0..4 {
        thread::sleep(Duration::from_millis(200));
        let _ = idle_cache.get(&"hot-key".to_string()); // reset idle timer
        idle_cache.run_pending_tasks();
    }

    println!("    After 800ms (hot-key accessed every 200ms, cold-key ignored):");
    println!("    hot-key:  {:?}  ← alive (kept resetting idle timer)", idle_cache.get(&"hot-key".to_string()));
    println!("    cold-key: {:?}  ← evicted (idle > 400ms)", idle_cache.get(&"cold-key".to_string()));

    println!("\n    moka eviction: TTL (absolute expiry), TTI (idle expiry), LFU (size limit).");
    println!("    In production: Redis EXPIRE, Memcached expiry, CDN max-age.\n");
}
