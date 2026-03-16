//! # Scaling Reads Pattern Demos
//!
//! This module demonstrates common patterns for scaling read-heavy workloads:
//! 1. Cache-Aside Pattern (Lazy Loading)
//! 2. Read-Through Cache
//! 3. Read Replicas Simulation
//! 4. Local + Distributed Cache Layers

use dashmap::DashMap;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Pattern 1: Cache-Aside (Lazy Loading)
// =============================================================================
// The most common caching pattern:
// - Check cache first
// - On miss, query database
// - Store result in cache

/// Simulated slow database
struct Database {
    data: HashMap<String, String>,
    query_count: AtomicU64,
    latency_ms: u64,
}

impl Database {
    fn new(latency_ms: u64) -> Self {
        let mut data = HashMap::new();
        // Pre-populate with some data
        for i in 0..1000 {
            data.insert(format!("user:{}", i), format!("User {} data", i));
        }
        Self {
            data,
            query_count: AtomicU64::new(0),
            latency_ms,
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.query_count.fetch_add(1, Ordering::SeqCst);
        // Simulate database latency
        std::thread::sleep(Duration::from_millis(self.latency_ms));
        self.data.get(key).cloned()
    }

    fn queries(&self) -> u64 {
        self.query_count.load(Ordering::SeqCst)
    }
}

/// Cache with TTL support
struct CacheEntry {
    value: String,
    expires_at: Instant,
}

struct CacheAside {
    cache: DashMap<String, CacheEntry>,
    db: Arc<Database>,
    ttl: Duration,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl CacheAside {
    fn new(db: Arc<Database>, ttl: Duration) -> Self {
        Self {
            cache: DashMap::new(),
            db,
            ttl,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }

    /// Cache-Aside read pattern
    fn get(&self, key: &str) -> Option<String> {
        // Step 1: Check cache
        if let Some(entry) = self.cache.get(key) {
            if entry.expires_at > Instant::now() {
                self.cache_hits.fetch_add(1, Ordering::SeqCst);
                return Some(entry.value.clone());
            }
            // Expired - remove and fall through to DB
            drop(entry);
            self.cache.remove(key);
        }

        // Step 2: Cache miss - query database
        self.cache_misses.fetch_add(1, Ordering::SeqCst);
        let value = self.db.get(key)?;

        // Step 3: Populate cache
        self.cache.insert(
            key.to_string(),
            CacheEntry {
                value: value.clone(),
                expires_at: Instant::now() + self.ttl,
            },
        );

        Some(value)
    }

    fn stats(&self) -> (u64, u64, f64) {
        let hits = self.cache_hits.load(Ordering::SeqCst);
        let misses = self.cache_misses.load(Ordering::SeqCst);
        let ratio = if hits + misses > 0 {
            hits as f64 / (hits + misses) as f64 * 100.0
        } else {
            0.0
        };
        (hits, misses, ratio)
    }
}

// =============================================================================
// Pattern 2: Read Replicas
// =============================================================================
// Distribute reads across multiple replicas
// Writes go to primary, reads from replicas

struct ReplicaSet {
    primary: Arc<RwLock<HashMap<String, String>>>,
    replicas: Vec<Arc<RwLock<HashMap<String, String>>>>,
    replication_lag_ms: u64,
    read_count: AtomicU64,
}

impl ReplicaSet {
    fn new(num_replicas: usize, replication_lag_ms: u64) -> Self {
        let primary = Arc::new(RwLock::new(HashMap::new()));
        let replicas: Vec<_> = (0..num_replicas)
            .map(|_| Arc::new(RwLock::new(HashMap::new())))
            .collect();

        Self {
            primary,
            replicas,
            replication_lag_ms,
            read_count: AtomicU64::new(0),
        }
    }

    /// Write to primary (synchronous for this demo)
    fn write(&self, key: String, value: String) {
        // Write to primary
        self.primary.write().insert(key.clone(), value.clone());

        // Async replication to replicas (simulated)
        for replica in &self.replicas {
            let replica = Arc::clone(replica);
            let key = key.clone();
            let value = value.clone();
            let lag = self.replication_lag_ms;
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(lag));
                replica.write().insert(key, value);
            });
        }
    }

    /// Read from random replica (load balanced)
    fn read(&self, key: &str) -> Option<String> {
        self.read_count.fetch_add(1, Ordering::SeqCst);

        // Round-robin or random selection
        let idx = rand::thread_rng().gen_range(0..self.replicas.len());
        self.replicas[idx].read().get(key).cloned()
    }

    /// Read from primary (for read-your-writes consistency)
    fn read_primary(&self, key: &str) -> Option<String> {
        self.primary.read().get(key).cloned()
    }
}

// =============================================================================
// Pattern 3: Multi-Layer Cache
// =============================================================================
// L1: Local in-process cache (fastest, smallest)
// L2: Distributed cache like Redis (fast, shared)
// L3: Database (slowest, authoritative)

struct L1Cache {
    data: DashMap<String, (String, Instant)>,
    ttl: Duration,
}

impl L1Cache {
    fn new(ttl: Duration) -> Self {
        Self {
            data: DashMap::new(),
            ttl,
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(|entry| {
            if entry.1 > Instant::now() {
                Some(entry.0.clone())
            } else {
                None
            }
        })
    }

    fn set(&self, key: String, value: String) {
        self.data.insert(key, (value, Instant::now() + self.ttl));
    }
}

struct L2Cache {
    data: DashMap<String, (String, Instant)>,
    ttl: Duration,
    latency_ms: u64,
}

impl L2Cache {
    fn new(ttl: Duration, latency_ms: u64) -> Self {
        Self {
            data: DashMap::new(),
            ttl,
            latency_ms,
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        std::thread::sleep(Duration::from_millis(self.latency_ms));
        self.data.get(key).and_then(|entry| {
            if entry.1 > Instant::now() {
                Some(entry.0.clone())
            } else {
                None
            }
        })
    }

    fn set(&self, key: String, value: String) {
        self.data.insert(key, (value, Instant::now() + self.ttl));
    }
}

struct MultiLayerCache {
    l1: L1Cache,
    l2: L2Cache,
    db: Arc<Database>,
    l1_hits: AtomicU64,
    l2_hits: AtomicU64,
    db_hits: AtomicU64,
}

impl MultiLayerCache {
    fn new(db: Arc<Database>) -> Self {
        Self {
            l1: L1Cache::new(Duration::from_secs(1)), // Very short TTL for L1
            l2: L2Cache::new(Duration::from_secs(60), 2), // Longer TTL, 2ms latency
            db,
            l1_hits: AtomicU64::new(0),
            l2_hits: AtomicU64::new(0),
            db_hits: AtomicU64::new(0),
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        // Try L1 first (in-process, ~0.001ms)
        if let Some(value) = self.l1.get(key) {
            self.l1_hits.fetch_add(1, Ordering::SeqCst);
            return Some(value);
        }

        // Try L2 (Redis-like, ~2ms)
        if let Some(value) = self.l2.get(key) {
            self.l2_hits.fetch_add(1, Ordering::SeqCst);
            // Populate L1
            self.l1.set(key.to_string(), value.clone());
            return Some(value);
        }

        // Fall through to database (~50ms)
        self.db_hits.fetch_add(1, Ordering::SeqCst);
        let value = self.db.get(key)?;

        // Populate both caches
        self.l2.set(key.to_string(), value.clone());
        self.l1.set(key.to_string(), value.clone());

        Some(value)
    }

    fn stats(&self) -> (u64, u64, u64) {
        (
            self.l1_hits.load(Ordering::SeqCst),
            self.l2_hits.load(Ordering::SeqCst),
            self.db_hits.load(Ordering::SeqCst),
        )
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("=== Scaling Reads Pattern Demos ===\n");

    // Demo 1: Cache-Aside Pattern
    println!("--- Pattern 1: Cache-Aside ---");
    let db = Arc::new(Database::new(10)); // 10ms DB latency
    let cache = CacheAside::new(Arc::clone(&db), Duration::from_secs(5));

    // First access - cache miss
    let start = Instant::now();
    let _ = cache.get("user:42");
    println!("First access (miss): {:?}", start.elapsed());

    // Second access - cache hit
    let start = Instant::now();
    let _ = cache.get("user:42");
    println!("Second access (hit): {:?}", start.elapsed());

    // Simulate workload
    for _ in 0..100 {
        let key = format!("user:{}", rand::thread_rng().gen_range(0..50));
        let _ = cache.get(&key);
    }
    let (hits, misses, ratio) = cache.stats();
    println!(
        "After 100 reads: {} hits, {} misses, {:.1}% hit rate",
        hits, misses, ratio
    );
    println!("Database queries: {}", db.queries());
    println!();

    // Demo 2: Read Replicas
    println!("--- Pattern 2: Read Replicas ---");
    let replica_set = ReplicaSet::new(3, 50); // 3 replicas, 50ms lag

    // Write to primary
    replica_set.write("key1".to_string(), "value1".to_string());

    // Immediate read from primary (consistent)
    println!(
        "Read from primary immediately: {:?}",
        replica_set.read_primary("key1")
    );

    // Read from replica immediately (may be stale due to lag)
    println!(
        "Read from replica immediately: {:?}",
        replica_set.read("key1")
    );

    // Wait for replication
    std::thread::sleep(Duration::from_millis(100));
    println!(
        "Read from replica after replication: {:?}",
        replica_set.read("key1")
    );
    println!();

    // Demo 3: Multi-Layer Cache
    println!("--- Pattern 3: Multi-Layer Cache ---");
    let db2 = Arc::new(Database::new(50)); // 50ms DB latency
    let multi_cache = MultiLayerCache::new(db2);

    // First access - goes to DB
    let start = Instant::now();
    let _ = multi_cache.get("user:1");
    println!("First access (DB): {:?}", start.elapsed());

    // Second access - L2 hit (Redis)
    let start = Instant::now();
    let _ = multi_cache.get("user:1");
    println!("Second access (L2): {:?}", start.elapsed());

    // Third access - L1 hit (local)
    let start = Instant::now();
    let _ = multi_cache.get("user:1");
    println!("Third access (L1): {:?}", start.elapsed());

    // Simulate workload
    for _ in 0..200 {
        let key = format!("user:{}", rand::thread_rng().gen_range(0..20));
        let _ = multi_cache.get(&key);
    }
    let (l1, l2, db_hits) = multi_cache.stats();
    println!("\nAfter 200 reads:");
    println!("  L1 (local) hits: {}", l1);
    println!("  L2 (Redis) hits: {}", l2);
    println!("  Database hits: {}", db_hits);

    println!("\n=== Key Takeaways ===");
    println!("1. Cache-Aside: App controls caching logic, good for read-heavy");
    println!("2. Read Replicas: Scale reads horizontally, watch for lag");
    println!("3. Multi-Layer: L1 for hot data, L2 for warm, DB for cold");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_aside_hit() {
        let db = Arc::new(Database::new(1));
        let cache = CacheAside::new(db, Duration::from_secs(60));

        // First call - miss
        let v1 = cache.get("user:1");
        assert!(v1.is_some());

        // Second call - hit
        let v2 = cache.get("user:1");
        assert_eq!(v1, v2);

        let (hits, misses, _) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_replica_consistency() {
        let rs = ReplicaSet::new(2, 10);
        rs.write("k".to_string(), "v".to_string());

        // Primary should have it immediately
        assert_eq!(rs.read_primary("k"), Some("v".to_string()));

        // Wait for replication
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(rs.read("k"), Some("v".to_string()));
    }
}
