use crate::store::{Cache, Store};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// =============================================================================
// Cache Stampede (Thundering Herd)
//
//   Problem: popular key expires → N threads miss → N concurrent DB queries
//
//     Time 0:  cache expires "product:hot"
//     T1: miss → DB                    ← DB load = 1
//     T2: miss → DB                    ← DB load = 2
//     ...
//     T100: miss → DB                  ← DB load = 100 ← could kill DB
//
//   Fix: SETNX lock — only 1 thread rebuilds, others wait for cache
//
//     T1: miss → SETNX lock → acquired → DB → fill cache → unlock
//     T2: miss → SETNX lock → FAILED → spin-wait → cache.get → HIT
//     T3: miss → SETNX lock → FAILED → spin-wait → cache.get → HIT
//     = 1 DB query total
//
//   SETNX details:
//     SET lock:product:hot 1 NX EX 5
//       NX   = only set if not exists (atomic acquire)
//       EX 5 = auto-expire in 5s (safety net if holder crashes)
//
//   Lock TTL must be > rebuild time:
//     If rebuild takes 3s but lock TTL is 2s → lock expires mid-rebuild
//     → another thread acquires → 2 concurrent rebuilds → defeats purpose
//     Rule: lock TTL = 2× max expected rebuild time
//
//   Production alternative: "early refresh" / "probabilistic expiry"
//     Instead of locking, refresh the cache key BEFORE it expires.
//     e.g., TTL=60s, at TTL<10s a random reader triggers async refresh.
//     No locking needed, but more complex.
// =============================================================================

/// StampedeGuard wraps a Store and ensures only 1 thread rebuilds on miss.
struct StampedeGuard {
    store: Arc<Store>,
    db_hits: AtomicI32,
}

impl StampedeGuard {
    fn new(store: Arc<Store>) -> Self {
        Self { store, db_hits: AtomicI32::new(0) }
    }

    /// Read with SETNX-based stampede protection.
    /// On miss: try to acquire rebuild lock.
    ///   Won  → query DB, fill cache, release lock.
    ///   Lost → spin-wait until winner fills cache.
    fn get(&self, cache: &Cache, key: &str) -> Option<String> {
        // Fast path: cache hit
        if let Some(v) = cache.get(key) { return Some(v); }

        let lock_key = format!("lock:{}", key);

        if cache.try_lock(&lock_key, 10) {
            // Won the lock → I rebuild
            thread::sleep(Duration::from_millis(50));  // simulate expensive query
            let val = self.store.db.get(key)?;
            cache.set(key, &val, 60);
            cache.del(&lock_key);
            self.db_hits.fetch_add(1, Ordering::SeqCst);
            Some(val)
        } else {
            // Lost the lock → someone else is rebuilding. Wait for them.
            for _ in 0..50 {
                thread::sleep(Duration::from_millis(10));
                if let Some(v) = cache.get(key) { return Some(v); }
            }
            // Timeout: lock holder may have crashed.
            // Lock auto-expires (TTL), next request retries.
            None
        }
    }

    /// Read WITHOUT stampede protection (for comparison).
    fn get_naive(&self, cache: &Cache, key: &str) -> Option<String> {
        if let Some(v) = cache.get(key) { return Some(v); }
        thread::sleep(Duration::from_millis(50));  // simulate expensive query
        let val = self.store.db.get(key)?;
        cache.set(key, &val, 60);
        self.db_hits.fetch_add(1, Ordering::SeqCst);
        Some(val)
    }

    fn reset_hits(&self) { self.db_hits.store(0, Ordering::SeqCst); }
    fn hits(&self) -> i32 { self.db_hits.load(Ordering::SeqCst) }
}

pub fn demo() {
    println!("\n  ═══ Cache Stampede ═══\n");

    let store = Arc::new(Store::new());
    let guard = Arc::new(StampedeGuard::new(Arc::clone(&store)));

    // Seed DB with an expensive-to-compute value
    store.db.set("product:hot", r#"{"name":"Widget","price":9.99}"#);

    // ── Without lock: all threads hit DB ──

    println!("    Without lock (10 threads all miss):\n");
    store.cache.del("product:hot");
    guard.reset_hits();

    let mut handles = vec![];
    for _ in 0..10 {
        let s = Arc::clone(&store);
        let g = Arc::clone(&guard);
        handles.push(thread::spawn(move || {
            let cache = s.new_cache_conn();
            g.get_naive(&cache, "product:hot");
        }));
    }
    for h in handles { h.join().unwrap(); }
    println!("      DB hits: {} (all 10 threads hit DB)\n", guard.hits());

    // ── With SETNX lock: only 1 thread rebuilds ──

    println!("    With SETNX lock (10 threads, 1 rebuilds):\n");
    store.cache.del("product:hot");
    guard.reset_hits();

    let mut handles = vec![];
    for _ in 0..10 {
        let s = Arc::clone(&store);
        let g = Arc::clone(&guard);
        handles.push(thread::spawn(move || {
            let cache = s.new_cache_conn();
            g.get(&cache, "product:hot");
        }));
    }
    for h in handles { h.join().unwrap(); }
    println!("      DB hits: {} (only 1 thread hit DB)\n", guard.hits());
}
