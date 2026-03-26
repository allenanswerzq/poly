use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;

// =============================================================================
// Multi-Layer Caching (L1/L2)
//
// L1 = in-process HashMap (100ns, per-server, small, short TTL)
// L2 = shared cache like Redis (1ms, shared across servers, large, longer TTL)
// DB = database (50ms, source of truth)
//
// Request → L1? → L2? → DB
// Each layer absorbs 90-99% of traffic from the layer above.
// =============================================================================

struct L1Cache {
    data: HashMap<String, (String, Instant)>,
    ttl: Duration,
    hits: usize,
    misses: usize,
}

impl L1Cache {
    fn new(ttl: Duration) -> Self {
        Self { data: HashMap::new(), ttl, hits: 0, misses: 0 }
    }

    fn get(&mut self, key: &str) -> Option<String> {
        if let Some((val, inserted_at)) = self.data.get(key) {
            if inserted_at.elapsed() < self.ttl {
                self.hits += 1;
                return Some(val.clone());
            }
            // expired
        }
        self.misses += 1;
        None
    }

    fn set(&mut self, key: &str, val: &str) {
        self.data.insert(key.to_string(), (val.to_string(), Instant::now()));
    }
}

pub fn demo() {
    println!("\n  ═══ Multi-Layer Caching (L1/L2) ═══\n");

    // L1: local, 500ms TTL (very short to limit staleness)
    let mut l1 = L1Cache::new(Duration::from_millis(500));

    // L2: shared cache (simulated with moka), 5s TTL
    let l2: Cache<String, String> = Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(5))
        .build();

    // DB: source of truth
    let mut db: HashMap<String, String> = HashMap::new();
    db.insert("user:1".into(), r#"{"name":"Alice"}"#.into());
    db.insert("user:2".into(), r#"{"name":"Bob"}"#.into());

    let mut db_queries = 0;
    let mut l2_queries = 0;

    // Multi-layer get function
    let mut multi_get = |key: &str| -> String {
        // L1 check (100ns, in-process)
        if let Some(val) = l1.get(key) {
            return val;
        }

        // L2 check (1ms, shared)
        l2_queries += 1;
        if let Some(val) = l2.get(&key.to_string()) {
            l1.set(key, &val); // promote to L1
            return val;
        }

        // DB query (50ms, source of truth)
        db_queries += 1;
        thread::sleep(Duration::from_millis(10)); // simulate DB
        let val = db.get(key).cloned().unwrap_or_default();
        l2.insert(key.to_string(), val.clone()); // populate L2
        l1.set(key, &val);                        // populate L1
        val
    };

    // Simulate 10 requests for the same key
    println!("    10 requests for user:1:\n");
    for i in 0..10 {
        let start = Instant::now();
        let val = multi_get("user:1");
        let elapsed = start.elapsed();
        let source = if i == 0 { "DB (cold)" }
            else if elapsed.as_nanos() < 100_000 { "L1 (100ns)" }
            else { "L2 (1ms)" };
        println!("    req {}: {} [{:?}] ← {}", i + 1, val, elapsed, source);
    }

    // Wait for L1 to expire, show L2 takes over
    println!("\n    Wait 600ms (L1 TTL=500ms expires, L2 still valid)...\n");
    thread::sleep(Duration::from_millis(600));

    let start = Instant::now();
    let _ = multi_get("user:1");
    println!("    req 11: [{:?}] ← L2 (L1 expired, L2 still fresh)", start.elapsed());

    // L1 repopulated from L2
    let start = Instant::now();
    let _ = multi_get("user:1");
    println!("    req 12: [{:?}] ← L1 (repopulated from L2)\n", start.elapsed());

    println!("    Stats:");
    println!("      L1 hits: {}, L1 misses: {}", l1.hits, l1.misses);
    println!("      L2 queries: {}, DB queries: {}", l2_queries, db_queries);
    println!("      L1 absorbed {}% of requests\n",
        l1.hits * 100 / (l1.hits + l1.misses).max(1));
}
