use crate::store::{Cache, Store};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// =============================================================================
// Cache-Aside (Lazy Loading)
//
//   Read:  cache → miss → DB → populate cache → return
//   Write: update DB → invalidate cache (DEL, not SET)
//
//   Why DEL on write (not SET)?
//     If you SET the new value, you need the lock from write-through.
//     DEL is simpler: the next reader will populate the correct value.
//     Trade-off: 1 extra cache miss after every write.
//
//   Race condition (inherent — cannot be fully avoided without locks):
//
//     T1: cache miss → DB.get("k") returns "A"
//           ← T2: DB.set("k", "B") → cache.del("k")
//     T1: cache.set("k", "A")                    ← STALE!
//
//     T1 started its DB read before T2's write, gets the old value,
//     then backfills cache AFTER T2 already invalidated it.
//     Cache now holds stale "A" while DB has "B".
//
//   Mitigation:
//     Short TTL on cache → stale data auto-expires.
//     This is why cache-aside ALWAYS uses a TTL. Never cache forever.
//     The staleness window = max(read_latency, TTL).
//
//   When to use:
//     - Read-heavy workloads (90%+ reads)
//     - Acceptable to have brief staleness (seconds, not minutes)
//     - Simplest pattern — good default choice
// =============================================================================

/// Cache-Aside wraps a Store and provides get/set with the lazy-loading pattern.
struct CacheAside {
    store: Arc<Store>,
    db_reads: AtomicI32,
}

impl CacheAside {
    fn new(store: Arc<Store>) -> Self {
        Self { store, db_reads: AtomicI32::new(0) }
    }

    /// Read path: cache → miss → DB → populate cache
    fn get(&self, key: &str) -> Option<String> {
        self.get_with(&self.store.cache, key)
    }

    /// Same read path but with a custom Redis connection (for multi-threaded use)
    fn get_with(&self, cache: &Cache, key: &str) -> Option<String> {
        if let Some(val) = cache.get(key) {
            return Some(val);                  // HIT — fast path
        }
        self.db_reads.fetch_add(1, Ordering::Relaxed);
        let val = self.store.db.get(key)?;
        cache.set(key, &val, 60);              // populate with TTL (never forever!)
        Some(val)
    }

    /// Write path: update DB → DEL cache (not SET!)
    fn set(&self, key: &str, value: &str) {
        self.store.db.set(key, value);         // 1. DB is source of truth
        self.store.cache.del(key);             // 2. Invalidate — next read re-fills
    }

    fn db_read_count(&self) -> i32 {
        self.db_reads.load(Ordering::Relaxed)
    }
}

pub fn demo() {
    println!("\n  ═══ Cache-Aside ═══\n");

    let store = Arc::new(Store::new());
    let ca = Arc::new(CacheAside::new(Arc::clone(&store)));

    // Seed the database
    store.db.set("user:1", r#"{"name":"Alice"}"#);
    store.db.set("user:2", r#"{"name":"Bob"}"#);
    store.db.set("user:3", r#"{"name":"Charlie"}"#);

    // ── Cold cache: all misses ──

    println!("    Cold cache (first access):\n");
    for key in &["user:1", "user:2", "user:1"] {
        let before = ca.db_read_count();
        let val = ca.get(key).unwrap_or_default();
        let hit = ca.db_read_count() == before;
        println!("      GET {} → {} [{}]", key, val, if hit { "HIT" } else { "MISS → DB" });
    }

    // ── Warm cache: all hits ──

    println!("\n    Warm cache (second access):\n");
    for key in &["user:1", "user:2", "user:3"] {
        let before = ca.db_read_count();
        let val = ca.get(key).unwrap_or_default();
        let hit = ca.db_read_count() == before;
        println!("      GET {} → {} [{}]", key, val, if hit { "HIT" } else { "MISS → DB" });
    }

    // ── Write: DB update → cache invalidation ──

    println!("\n    Write: DB.set → cache.del → next read re-fills:\n");
    ca.set("user:1", r#"{"name":"Alicia"}"#);
    let val = ca.get("user:1").unwrap_or_default();
    println!("      GET user:1 → {} (fresh from DB after invalidation)", val);
    println!("      Total DB reads: {}", ca.db_read_count());

    // ── Race condition: stale backfill ──
    //
    //   T1: cache miss → sleeps (simulating slow DB read) → gets OLD value
    //   T2: writes NEW value → deletes cache
    //   T1: wakes up → backfills cache with OLD value → STALE
    //

    println!("\n    ── Race: stale backfill after invalidation ──\n");

    store.db.set("product:1", "v1-original");
    store.cache.del("product:1");

    let stale_detected = Arc::new(AtomicI32::new(0));
    let s1 = Arc::clone(&store);
    let sd = Arc::clone(&stale_detected);

    // T1: slow reader — reads DB, sleeps, then backfills
    let reader = thread::spawn(move || {
        let cache = s1.new_cache_conn();
        // 1. Cache miss
        assert!(cache.get("product:1").is_none());
        // 2. Read DB (gets "v1-original")
        let old_val = s1.db.get("product:1").unwrap();
        // 3. Simulate slow network/processing
        thread::sleep(Duration::from_millis(100));
        // 4. Backfill cache with stale value
        cache.set("product:1", &old_val, 60);
        // Check: is cache now stale vs DB?
        let cached = cache.get("product:1").unwrap();
        let current_db = s1.db.get("product:1").unwrap();
        if cached != current_db {
            sd.fetch_add(1, Ordering::Relaxed);
        }
        (cached, current_db)
    });

    // T2: fast writer — updates DB + invalidates cache while T1 is sleeping
    thread::sleep(Duration::from_millis(30));
    ca.set("product:1", "v2-updated");

    let (cached, db_val) = reader.join().unwrap();
    let stale = stale_detected.load(Ordering::Relaxed);
    println!("      T1 backfilled cache = {:?}", cached);
    println!("      DB has              = {:?}", db_val);
    println!("      Stale? {} (cache has old value after invalidation)", stale > 0);
    println!("      Fix: TTL ensures staleness auto-expires (60s max)\n");
}
