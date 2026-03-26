use crate::store::{Cache, Store};
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// =============================================================================
// Hot Key + L1 Local Cache
//
//   Problem: one key gets 100k QPS → single Redis shard bottleneck
//   Fix:     L1 in-process cache absorbs most reads, only refresh from Redis
//
//         ┌──────────── App Process ────────────┐
//         │  L1 (DashMap, 5ms TTL)              │
//         │     ↓ miss                          │
//         │  L2 (Redis)                         │
//         │     ↓ miss                          │
//         │  DB (SQLite)                        │
//         └─────────────────────────────────────┘
//
//   L1 absorbs 90-99% of reads. Only ~1% leak to Redis.
//
//   Trade-off: L1 has a staleness window = L1 TTL.
//     Write happens → L2 updated → L1 still serves old value until its TTL.
//     5ms L1 TTL = max 5ms stale. Acceptable for hot read-heavy keys.
//     Do NOT use for data that must be immediately consistent after writes.
// =============================================================================

/// HotKeyCache: L1 in-process cache over L2 Redis.
/// Each app server has its own L1. L2 (Redis) is shared.
struct HotKeyCache {
    store: Arc<Store>,
    l1: DashMap<String, (String, Instant)>,
    l1_ttl: Duration,
    l1_hits: AtomicU64,
    l2_hits: AtomicU64,
}

impl HotKeyCache {
    fn new(store: Arc<Store>, l1_ttl_ms: u64) -> Self {
        Self {
            store,
            l1: DashMap::new(),
            l1_ttl: Duration::from_millis(l1_ttl_ms),
            l1_hits: AtomicU64::new(0),
            l2_hits: AtomicU64::new(0),
        }
    }

    /// Read: L1 → L2 (Redis) → DB
    fn get(&self, cache: &Cache, key: &str) -> Option<String> {
        // L1 check (in-process, zero network)
        if let Some(entry) = self.l1.get(key) {
            if entry.1.elapsed() < self.l1_ttl {
                self.l1_hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.0.clone());
            }
            drop(entry);
            self.l1.remove(key);
        }
        // L1 miss → L2 (Redis)
        if let Some(val) = cache.get(key) {
            self.l2_hits.fetch_add(1, Ordering::Relaxed);
            self.l1.insert(key.to_string(), (val.clone(), Instant::now()));
            return Some(val);
        }
        // L2 miss → DB (rare)
        let val = self.store.db.get(key)?;
        cache.set(key, &val, 300);
        self.l1.insert(key.to_string(), (val.clone(), Instant::now()));
        Some(val)
    }

    fn stats(&self) -> (u64, u64) {
        (self.l1_hits.load(Ordering::Relaxed), self.l2_hits.load(Ordering::Relaxed))
    }
}

pub fn demo() {
    println!("\n  ═══ Hot Key + L1 Local Cache ═══\n");

    let store = Arc::new(Store::new());
    let hk = Arc::new(HotKeyCache::new(Arc::clone(&store), 5));  // 5ms L1 TTL

    // Seed the hot key in both DB and Redis
    store.db.set("trending:1", r#"{"topic":"Breaking News","views":1000000}"#);
    store.cache.set("trending:1", r#"{"topic":"Breaking News","views":1000000}"#, 300);

    // Simulate 10 threads × 1000 reads on the same key
    let total_reads = 10_000u64;
    println!("    Simulating {} reads on a single hot key:", total_reads);

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..10 {
        let s = Arc::clone(&store);
        let h = Arc::clone(&hk);
        handles.push(thread::spawn(move || {
            let cache = s.new_cache_conn();
            for _ in 0..1000 {
                let _ = h.get(&cache, "trending:1");
            }
        }));
    }
    for h in handles { h.join().unwrap(); }
    let elapsed = start.elapsed();

    let (l1_hits, l2_hits) = hk.stats();
    let l1_pct = l1_hits as f64 / total_reads as f64 * 100.0;

    println!("    L1 hits: {} ({:.1}%)", l1_hits, l1_pct);
    println!("    L2 hits: {} ({:.1}%)", l2_hits, 100.0 - l1_pct);
    println!("    Total:   {} reads in {:?}", total_reads, elapsed);
    println!("    L1 absorbed {:.0}x the Redis load\n",
        if l2_hits > 0 { l1_hits as f64 / l2_hits as f64 } else { f64::INFINITY });
}
