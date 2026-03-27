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
//   Dirty tracking lives in Redis (not local memory):
//     - Survives app crash — flusher on restart knows which keys are dirty
//     - Works across N app servers (all SADD to the same Redis SET)
//
//   Race condition 1 — set() must be atomic:
//     SET value + SADD dirty are 2 commands. If app crashes between them:
//       Case A: value written, not in dirty set → never flushed, lost
//       Case B: in dirty set, value not written → flush reads stale data
//     Fix: Redis pipeline with MULTI/EXEC — both commands in 1 atomic tx.
//
//   Race condition 2 — flush() must not drop concurrent writes:
//     Naive: SMEMBERS → iterate → DEL dirty_set.
//     If a writer SADDs a new key between SMEMBERS and DEL,
//     that key is wiped from dirty set without being flushed → lost.
//     Fix: RENAME dirty_set → processing_set (atomic swap).
//          Writers SADD to dirty_set (new, empty). Flusher drains
//          processing_set. No writes are lost.
//
//   Production considerations:
//     - Redis AOF reduces but doesn't eliminate the loss window
//     - Flush interval = trade-off: shorter = less data loss, more DB writes
//     - Use for: analytics counters, view counts, non-critical writes
//     - Never use for: payments, orders, anything where loss = bad
// =============================================================================

const DIRTY_SET: &str = "dirty_keys";
const PROCESSING_SET: &str = "dirty_keys:processing";

/// WriteBehind wraps a Store and buffers writes in Redis with async DB flush.
/// All dirty tracking lives in Redis (SADD/RENAME/SMEMBERS).
struct WriteBehind {
    store: Arc<Store>,
}

impl WriteBehind {
    fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Write: atomic pipeline (MULTI/EXEC) of SET + SADD.
    /// Both commands succeed or neither does — no partial state.
    fn set(&self, key: &str, value: &str) {
        self.store.cache.set_and_sadd(key, value, 300, DIRTY_SET);
    }

    /// Read: cache → miss → DB → populate cache
    fn get(&self, key: &str) -> Option<String> {
        if let Some(v) = self.store.cache.get(key) { return Some(v); }
        let v = self.store.db.get(key)?;
        self.store.cache.set(key, &v, 300);
        Some(v)
    }

    /// Flush dirty keys to DB. Race-safe:
    ///   1. RENAME dirty_keys → dirty_keys:processing  (atomic swap)
    ///      Now writers SADD to a fresh dirty_keys, not the one we're reading.
    ///   2. SMEMBERS processing → for each: read cache → write DB
    ///   3. DEL processing
    /// No concurrent writes are lost because step 1 atomically moves
    /// the set away from writers.
    fn flush(&self) -> i32 {
        // Step 1: atomic swap — writers now write to a new dirty_keys
        if !self.store.cache.rename(DIRTY_SET, PROCESSING_SET) {
            return 0;  // dirty_keys doesn't exist → nothing to flush
        }
        // Step 2: drain the processing set into DB
        let keys = self.store.cache.smembers(PROCESSING_SET);
        let mut flushed = 0;
        for key in &keys {
            if let Some(val) = self.store.cache.get(key) {
                self.store.db.set(key, &val);
                flushed += 1;
            }
        }
        // Step 3: clean up
        self.store.cache.del(PROCESSING_SET);
        flushed
    }

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
