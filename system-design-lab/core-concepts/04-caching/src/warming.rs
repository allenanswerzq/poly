use crate::store::Store;
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// Cache Warming (Pre-loading)
//
//   Problem: cold start — after deploy/restart, cache is empty, DB hammered
//   Fix:     pre-load hot keys into cache before traffic hits
//
//   Pipeline trick: batch 100 SETs in 1 round-trip instead of 100 round-trips
//
//         ┌────────────┐ 1 PIPELINE  ┌────────┐
//         │ Warm Script ├────────────►│ Redis  │  100 keys in 1 RTT
//         └──────┬─────┘             └────────┘
//                │ read hot keys
//         ┌──────▼─────┐
//         │    DB      │
//         └────────────┘
// =============================================================================

/// CacheWarmer pre-loads hot keys from DB into Redis using pipeline batching.
struct CacheWarmer {
    store: Arc<Store>,
}

impl CacheWarmer {
    fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Warm cache with individual SETs — 1 round-trip per key (slow)
    fn warm_individual(&self, keys: &[(String, String)], ttl: u64) -> u128 {
        self.store.cache.flush();
        let start = Instant::now();
        for (key, val) in keys {
            self.store.cache.set(key, val, ttl);
        }
        start.elapsed().as_micros()
    }

    /// Warm cache with pipeline — 1 round-trip for ALL keys (fast)
    fn warm_pipeline(&self, keys: &[(String, String)], ttl: u64) -> u128 {
        self.store.cache.flush();
        let batch: Vec<(&str, &str)> = keys.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let start = Instant::now();
        self.store.cache.set_batch(&batch, ttl);
        start.elapsed().as_micros()
    }
}

pub fn demo() {
    println!("\n  ═══ Cache Warming ═══\n");

    let store = Arc::new(Store::new());
    let warmer = CacheWarmer::new(Arc::clone(&store));

    // Seed DB with "hot" data
    let mut entries: Vec<(String, String)> = Vec::new();
    for i in 0..100 {
        let key = format!("product:{}", i);
        let val = format!(r#"{{"id":{},"name":"Product {}","price":{:.2}}}"#, i, i, i as f64 * 1.5);
        store.db.set(&key, &val);
        entries.push((key, val));
    }
    println!("    Seeded {} products in DB", entries.len());

    let individual_us = warmer.warm_individual(&entries, 300);
    println!("    Individual SET × 100: {}µs", individual_us);

    let pipeline_us = warmer.warm_pipeline(&entries, 300);
    println!("    Pipeline  SET × 100: {}µs", pipeline_us);

    let speedup = if pipeline_us > 0 { individual_us / pipeline_us } else { 0 };
    println!("    Pipeline is ~{}x faster (1 RTT vs 100)\n", speedup);

    // Verify a sample
    println!("    Verify: product:0 → {:?}", store.cache.get("product:0"));
    println!("    Verify: product:99 → {:?}\n", store.cache.get("product:99"));
}
