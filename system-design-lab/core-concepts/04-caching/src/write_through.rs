use crate::store::{Cache, Store};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// =============================================================================
// Write-Through
//
//   Write: DB first → cache second (under distributed lock)
//   Read:  cache → miss → DB → populate cache
//
//   Race condition (naive, NO lock):
//
//     T1: DB.set("k", "A")                       ← DB = A
//     T2: DB.set("k", "B")                       ← DB = B
//     T2: cache.set("k", "B")                    ← cache = B
//     T1: cache.set("k", "A")                    ← cache = A, DB = B  ← STALE!
//
//   Fix: Redis distributed lock (SETNX)
//
//     Why not a local Mutex?  Because in production you have N app servers.
//     A local Mutex only protects one process. SETNX protects across ALL.
//
//     T1: SETNX lock:k → acquired
//     T1: DB.set("k","A") → cache.set("k","A") → DEL lock:k
//     T2: SETNX lock:k → acquired (T1 released)
//     T2: DB.set("k","B") → cache.set("k","B") → DEL lock:k
//     → cache = B, DB = B  ← CONSISTENT ✓
//
//   Lock TTL safety net:
//     If the lock holder crashes, the lock auto-expires after TTL.
//     DB is source of truth — next cache miss self-heals from DB.
//
//   SETNX = "SET if Not eXists" — Redis's built-in atomic CAS.
//     SET lock:user:1 1 NX EX 5
//       NX → only set if key doesn't exist (atomic acquire)
//       EX 5 → auto-expire in 5s (crash safety)
// =============================================================================

/// WriteThrough wraps a Store and does DB+cache writes under a Redis SETNX lock.
struct WriteThrough {
    store: Arc<Store>,
}

impl WriteThrough {
    fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Write: acquire distributed lock → DB first → cache second → release
    fn set(&self, key: &str, value: &str) {
        self.set_with(&self.store.cache, key, value);
    }

    /// Write: acquire distributed lock → DB first → cache second → release.
    /// Returns (cache_val, db_val) observed while still holding the lock.
    fn set_with(&self, cache: &Cache, key: &str, value: &str) -> (String, String) {
        let lock_key = format!("lock:{}", key);
        while !cache.try_lock(&lock_key, 5) {
            thread::sleep(Duration::from_millis(1));
        }
        self.store.db.set(key, value); // 1. DB first (source of truth)
        cache.set(key, value, 300); // 2. Cache second
                                    // Read back while still holding the lock — proves consistency
        let c = cache.get(key).unwrap_or_default();
        let d = self.store.db.get(key).unwrap_or_default();
        cache.del(&lock_key); // 3. Release lock
        (c, d)
    }

    /// Read: cache → miss → DB → populate (self-heal on miss)
    fn get(&self, key: &str) -> Option<String> {
        if let Some(v) = self.store.cache.get(key) {
            return Some(v);
        }
        let v = self.store.db.get(key)?;
        self.store.cache.set(key, &v, 300);
        Some(v)
    }
}

pub fn demo() {
    println!("\n  ═══ Write-Through ═══\n");

    let store = Arc::new(Store::new());
    let wt = Arc::new(WriteThrough::new(Arc::clone(&store)));

    // ── Basic operations ──

    wt.set("user:1", r#"{"name":"Alice"}"#);
    wt.set("user:2", r#"{"name":"Bob"}"#);

    println!("    SET user:1, user:2 (DB + cache, SETNX locked)");
    println!("    GET user:1 → {:?}", wt.get("user:1"));
    println!(
        "    consistent: cache={:?} == db={:?}",
        store.cache.get("user:1"),
        store.db.get("user:1")
    );

    // Update
    wt.set("user:1", r#"{"name":"Alicia"}"#);
    println!("\n    UPDATE user:1 → {:?}", wt.get("user:1"));

    // Cache eviction → self-healing read from DB
    store.cache.del("user:2");
    println!(
        "    DEL cache user:2 → read heals from DB: {:?}",
        wt.get("user:2")
    );

    // ── Show the race condition WITHOUT distributed lock ──

    println!("\n    ── Race: 5 threads × 100 writes, NO lock ──\n");

    let mismatches_naive = Arc::new(AtomicI32::new(0));
    let mut handles = vec![];
    for t in 0..5 {
        let s = Arc::clone(&store);
        let mis = Arc::clone(&mismatches_naive);
        handles.push(thread::spawn(move || {
            let cache = s.new_cache_conn();
            for i in 0..100 {
                let val = format!("t{}-v{}", t, i);
                // Naive: no lock → interleave between DB and cache write
                s.db.set("race", &val);
                cache.set("race", &val, 300);

                let c = cache.get("race").unwrap_or_default();
                let d = s.db.get("race").unwrap_or_default();
                if c != d {
                    mis.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!(
        "      Mismatches: {} (non-deterministic, often > 0)",
        mismatches_naive.load(Ordering::Relaxed)
    );

    // ── Same test WITH SETNX distributed lock ──

    println!("\n    ── Same test WITH SETNX distributed lock ──\n");

    store.cache.del("race");
    store.db.del("race");
    let mismatches_locked = Arc::new(AtomicI32::new(0));
    let mut handles = vec![];
    for t in 0..5 {
        let s = Arc::clone(&store);
        let wt2 = Arc::clone(&wt);
        let mis = Arc::clone(&mismatches_locked);
        handles.push(thread::spawn(move || {
            let cache = s.new_cache_conn();
            for i in 0..100 {
                let val = format!("t{}-v{}", t, i);
                let (c, d) = wt2.set_with(&cache, "race", &val);

                if c != d {
                    mis.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!(
        "      Mismatches: {} (always 0)\n",
        mismatches_locked.load(Ordering::Relaxed)
    );
}
