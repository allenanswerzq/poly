use crate::store::Store;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

// =============================================================================
// Cache Penetration
//
//   Problem: query for key that NEVER exists (not in cache NOR DB)
//            → every request is a cache miss + DB miss → DB hammered
//
//   Example: attacker sends GET /user/99999999 → DB scanned every time
//
//   Fix 1: "negative caching" — cache the NULL result with a short TTL
//
//         GET user:X → miss → DB → not found → cache "∅" (30s TTL)
//         GET user:X → hit "∅" → return 404 immediately (no DB)
//
//   Fix 2: Bloom filter (not shown here, but mentioned)
//         Before hitting DB, check a bloom filter.
//         If bloom says "definitely not in DB" → return 404, skip DB.
//         Bloom filters have false positives (say yes when no) but
//         never false negatives (never say no when yes).
//         Space-efficient: 1GB bloom can track 1 billion keys.
// =============================================================================

/// NegativeCache wraps a Store and caches "not found" results to prevent
/// repeated DB lookups for keys that don't exist.
struct NegativeCache {
    store: Arc<Store>,
    null_marker: &'static str,
    db_hits: AtomicI32,
}

impl NegativeCache {
    fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            null_marker: "\x00NULL",
            db_hits: AtomicI32::new(0),
        }
    }

    /// Read with negative caching: cache miss + DB miss → cache the absence
    fn get(&self, key: &str) -> Option<String> {
        // 1. Check cache
        if let Some(val) = self.store.cache.get(key) {
            if val == self.null_marker {
                return None; // cached negative → skip DB
            }
            return Some(val);
        }
        // 2. Cache miss → DB
        self.db_hits.fetch_add(1, Ordering::Relaxed);
        match self.store.db.get(key) {
            Some(val) => {
                self.store.cache.set(key, &val, 60); // normal cache
                Some(val)
            }
            None => {
                self.store.cache.set(key, self.null_marker, 30); // negative cache (short TTL)
                None
            }
        }
    }

    fn reset_hits(&self) {
        self.db_hits.store(0, Ordering::Relaxed);
    }
    fn hits(&self) -> i32 {
        self.db_hits.load(Ordering::Relaxed)
    }
}

pub fn demo() {
    println!("\n  ═══ Cache Penetration ═══\n");

    let store = Arc::new(Store::new());
    let nc = NegativeCache::new(Arc::clone(&store));

    // Seed DB with only a few users
    store.db.set("user:1", r#"{"name":"Alice"}"#);
    store.db.set("user:2", r#"{"name":"Bob"}"#);

    // ── Without negative caching: every lookup hits DB ──

    println!("    Without negative caching:");
    let mut raw_hits = 0;
    for _ in 0..100 {
        let _ = store.db.get("user:999"); // always misses, always queries
        raw_hits += 1;
    }
    println!("      100 lookups for missing key → {} DB hits\n", raw_hits);

    // ── With negative caching: 1 DB hit, 99 cache hits ──

    println!("    With negative caching:");
    nc.reset_hits();
    for _ in 0..100 {
        let _ = nc.get("user:999");
    }
    println!(
        "      100 lookups for missing key → {} DB hit(s)",
        nc.hits()
    );
    println!(
        "      Cached as {:?} (30s TTL)\n",
        store.cache.get("user:999").map(|v| if v == "\x00NULL" {
            "NULL marker".to_string()
        } else {
            v
        })
    );

    // ── Normal keys still work ──

    nc.reset_hits();
    let val = nc.get("user:1");
    println!(
        "    Existing key: user:1 → {:?} (DB hits: {})\n",
        val,
        nc.hits()
    );
}
