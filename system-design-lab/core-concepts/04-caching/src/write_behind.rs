use crate::store::Store;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// =============================================================================
// Write-Behind (Write-Back)
//
//   Write: cache only (instant return) → async flush to DB later
//   Read:  cache → miss → DB → cache
//
//   Key benefit: WRITE COALESCING
//     100 writes to same key in 1s → only 1 DB write (latest value)
//     1000 writes to 50 keys → 50 DB writes (not 1000)
//
//   Trade-off: crash between write and flush → DATA LOSS
//     The cache (Redis) is NOT the source of truth here.
//     If Redis dies before flush, unflushed writes are gone.
//
//         ┌──────┐  set()  ┌───────┐  flush()  ┌────┐
//         │Client├────────►│ Redis  ├──────────►│ DB │
//         └──────┘         └───────┘ (batched)  └────┘
//                        returns immediately
//
//   Production considerations:
//     - Redis AOF (appendonly yes) reduces but doesn't eliminate loss window
//     - Flush interval = trade-off between latency and data loss window
//     - Use for: analytics counters, view counts, non-critical writes
//     - Never use for: payments, orders, anything where loss = bad
// =============================================================================

/// WriteBehind wraps a Store and buffers writes in cache with async DB flush.
struct WriteBehind {
    store: Arc<Store>,
    dirty: dashmap::DashSet<String>,
}

impl WriteBehind {
    fn new(store: Arc<Store>) -> Self {
        Self { store, dirty: dashmap::DashSet::new() }
    }

    /// Write: cache only (instant) + mark key as dirty for future flush
    fn set(&self, key: &str, value: &str) {
        self.store.cache.set(key, value, 300);  // client returns here (fast)
        self.dirty.insert(key.to_string());      // mark for async flush
    }

    /// Read: cache → miss → DB → populate cache
    fn get(&self, key: &str) -> Option<String> {
        if let Some(v) = self.store.cache.get(key) { return Some(v); }
        let v = self.store.db.get(key)?;
        self.store.cache.set(key, &v, 300);
        Some(v)
    }

    /// Flush all dirty keys from cache → DB. Returns number flushed.
    /// In production this runs in a background loop on a timer.
    fn flush(&self) -> i32 {
        let mut flushed = 0;
        let keys: Vec<String> = self.dirty.iter().map(|k| k.clone()).collect();
        for key in &keys {
            if let Some(val) = self.store.cache.get(key) {
                self.store.db.set(key, &val);
                flushed += 1;
            }
        }
        self.dirty.clear();
        flushed
    }

    fn dirty_count(&self) -> usize {
        self.dirty.len()
    }
}

pub fn demo() {
    println!("\n  ═══ Write-Behind ═══\n");

    let store = Arc::new(Store::new());
    let wb = Arc::new(WriteBehind::new(Arc::clone(&store)));

    // ── Demonstrate write coalescing ──

    println!("    Write coalescing (100 writes to 5 keys):\n");

    for i in 0..100 {
        let key = format!("counter:{}", i % 5);
        wb.set(&key, &format!("{}", i));
    }
    println!("      Writes to cache: 100");
    println!("      Dirty keys:      {} (coalesced!)", wb.dirty_count());
    println!("      DB writes so far: {} (none until flush)", store.db.count());

    // ── Async flush: background thread drains dirty set → DB ──

    let wb2 = Arc::clone(&wb);
    let flusher = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));   // batch window
        wb2.flush()
    });

    let flushed = flusher.join().unwrap();
    println!("\n      After flush: DB writes={} (5, not 100)", flushed);

    // Verify consistency post-flush
    println!("\n      Verify cache == DB after flush:");
    for i in 0..5 {
        let key = format!("counter:{}", i);
        let c = store.cache.get(&key).unwrap_or_default();
        let d = store.db.get(&key).unwrap_or_default();
        println!("        {} → cache={}, db={}, match={}", key, c, d, c == d);
    }

    // ── Data loss scenario: write, then crash before flush ──

    println!("\n    ── Crash = data loss ──\n");

    wb.set("payment:999", r#"{"amount":500}"#);
    println!("      Wrote payment:999 to cache (not flushed)");
    println!("      cache = {:?}", store.cache.get("payment:999"));
    println!("      db    = {:?}", store.db.get("payment:999"));
    println!("      If Redis crashes now → payment is LOST");
    println!("      This is why write-behind is NEVER used for payments\n");
}
