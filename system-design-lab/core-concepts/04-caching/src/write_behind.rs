use crate::store::Store;
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
//     Redis dies before flush → unflushed writes are gone.
//
//         ┌──────┐  set()  ┌───────┐  flush()  ┌────┐
//         │Client├────────►│ Redis  ├──────────►│ DB │
//         └──────┘         └───────┘ (batched)  └────┘
//                        returns immediately
//
//   Dirty tracking: use a Redis SET, not local memory.
//     Why? If the app crashes, a local DashSet is gone — you don't even
//     know WHICH keys to flush. A Redis SET survives app restarts.
//     Multi-server: N app servers all SADD to the same Redis SET.
//     Flusher reads SMEMBERS → flushes → DEL dirty set.
//
//   Production considerations:
//     - Redis AOF (appendonly yes) reduces but doesn't eliminate loss window
//     - Flush interval = trade-off between latency and data loss window
//     - Use for: analytics counters, view counts, non-critical writes
//     - Never use for: payments, orders, anything where loss = bad
// =============================================================================

const DIRTY_SET: &str = "dirty_keys";

/// WriteBehind wraps a Store and buffers writes in Redis with async DB flush.
/// Dirty key tracking uses a Redis SET (SADD/SMEMBERS), NOT local memory.
struct WriteBehind {
    store: Arc<Store>,
}

impl WriteBehind {
    fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Write: cache SET + SADD to dirty set (both in Redis, survives app crash)
    fn set(&self, key: &str, value: &str) {
        self.store.cache.set(key, value, 300);     // cached value
        self.store.cache.sadd(DIRTY_SET, key);     // track dirty key in Redis
    }

    /// Read: cache → miss → DB → populate cache
    fn get(&self, key: &str) -> Option<String> {
        if let Some(v) = self.store.cache.get(key) { return Some(v); }
        let v = self.store.db.get(key)?;
        self.store.cache.set(key, &v, 300);
        Some(v)
    }

    /// Flush: SMEMBERS dirty set → read each from cache → write to DB → DEL set.
    /// Returns number of keys flushed.
    fn flush(&self) -> i32 {
        let keys = self.store.cache.smembers(DIRTY_SET);
        let mut flushed = 0;
        for key in &keys {
            if let Some(val) = self.store.cache.get(key) {
                self.store.db.set(key, &val);
                flushed += 1;
            }
        }
        self.store.cache.del(DIRTY_SET);           // clear dirty set
        flushed
    }

    /// How many keys are dirty (pending flush)?
    fn dirty_count(&self) -> i64 {
        self.store.cache.scard(DIRTY_SET)
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
    println!("      Dirty keys (Redis SCARD): {} (coalesced by SADD)", wb.dirty_count());
    println!("      DB writes so far: {} (none until flush)", store.db.count());

    // ── Async flush: background thread reads dirty set from Redis → DB ──

    let wb2 = Arc::clone(&wb);
    let flusher = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));   // batch window
        wb2.flush()
    });

    let flushed = flusher.join().unwrap();
    println!("\n      After flush: DB writes={} (5, not 100)", flushed);
    println!("      Dirty keys after flush: {}", wb.dirty_count());

    // Verify consistency post-flush
    println!("\n      Verify cache == DB after flush:");
    for i in 0..5 {
        let key = format!("counter:{}", i);
        let c = store.cache.get(&key).unwrap_or_default();
        let d = store.db.get(&key).unwrap_or_default();
        println!("        {} → cache={}, db={}, match={}", key, c, d, c == d);
    }

    // ── Data loss scenario ──

    println!("\n    ── Crash = data loss ──\n");

    wb.set("payment:999", r#"{"amount":500}"#);
    println!("      Wrote payment:999 to cache (dirty set tracks it in Redis)");
    println!("      cache = {:?}", store.cache.get("payment:999"));
    println!("      db    = {:?}", store.db.get("payment:999"));
    println!("      dirty = {:?}", store.cache.smembers(DIRTY_SET));
    println!("      If REDIS crashes → both value AND dirty set are lost");
    println!("      This is why write-behind is NEVER used for payments\n");
}
