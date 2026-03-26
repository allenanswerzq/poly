use moka::sync::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::thread;

// =============================================================================
// Hot Key — one key gets disproportionate traffic.
// Single Redis can handle ~100K ops/s. A viral tweet gets 1M reads/s.
// Solution: local in-process cache (L1) absorbs the hot key.
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Hot Key Problem ═══\n");

    // Simulate a shared cache (like Redis) with a throughput limit
    let redis_ops = Arc::new(AtomicUsize::new(0));
    let redis_data: Arc<Cache<String, String>> = Arc::new(Cache::builder()
        .max_capacity(1000)
        .build());

    // Populate Redis with the viral tweet
    redis_data.insert("tweet:viral".into(), r#"{"text":"This tweet went viral!","likes":5000000}"#.into());

    // ── Without L1: every request hits Redis ──
    println!("    Without L1 cache (all requests hit Redis):\n");

    let ops = Arc::clone(&redis_ops);
    let data = Arc::clone(&redis_data);

    let start = Instant::now();
    let mut handles = vec![];

    // 10 "app servers" each sending 1000 requests for the viral tweet
    for server_id in 0..10 {
        let ops = Arc::clone(&ops);
        let data = Arc::clone(&data);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                ops.fetch_add(1, Ordering::Relaxed);
                let _ = data.get(&"tweet:viral".to_string());
            }
        }));
    }
    for h in handles { h.join().unwrap(); }
    let total_ops = ops.load(Ordering::Relaxed);
    println!("    10 servers × 1000 req = {} Redis ops in {:?}",
        total_ops, start.elapsed());
    println!("    Redis at 100K ops/s: would need {:.1}s to process\n",
        total_ops as f64 / 100_000.0);

    // ── With L1: local cache absorbs hot key ──
    println!("    With L1 local cache (hot key cached per-server):\n");

    let redis_ops2 = Arc::new(AtomicUsize::new(0));
    let data2 = Arc::clone(&redis_data);

    let start = Instant::now();
    let mut handles = vec![];

    for server_id in 0..10 {
        let redis_ops = Arc::clone(&redis_ops2);
        let data = Arc::clone(&data2);
        handles.push(thread::spawn(move || {
            // Each server has its own L1 cache (HashMap, 5s TTL simulated)
            let mut l1: HashMap<String, String> = HashMap::new();

            for i in 0..1000 {
                let key = "tweet:viral";

                // Check L1 first
                if let Some(val) = l1.get(key) {
                    // L1 hit — no Redis call
                    continue;
                }

                // L1 miss — fetch from Redis (once per server)
                redis_ops.fetch_add(1, Ordering::Relaxed);
                if let Some(val) = data.get(&key.to_string()) {
                    l1.insert(key.to_string(), val);
                }
            }
        }));
    }
    for h in handles { h.join().unwrap(); }
    let total_ops2 = redis_ops2.load(Ordering::Relaxed);
    println!("    10 servers × 1000 req: only {} Redis ops (1 per server)",
        total_ops2);
    println!("    Reduction: {:.0}x fewer Redis calls", total_ops as f64 / total_ops2 as f64);
    println!("    L1 absorbed {:.1}% of requests\n",
        (1.0 - total_ops2 as f64 / total_ops as f64) * 100.0);

    println!("    Hot key solutions:");
    println!("    1. L1 local cache (shown above) — 1000x reduction");
    println!("    2. Replicate key across shards: tweet:viral:shard0..N");
    println!("    3. Client-side cache with 1-5s TTL\n");
}
