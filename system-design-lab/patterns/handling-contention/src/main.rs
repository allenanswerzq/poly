//! # Handling Contention Pattern Demos
//!
//! This module demonstrates patterns for handling concurrent access to shared resources:
//! 1. Optimistic Locking (Version-based)
//! 2. Pessimistic Locking (Mutex)
//! 3. Atomic Operations (Compare-and-Swap)
//! 4. Queue-Based Serialization
//! 5. Hot Key Mitigation

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// =============================================================================
// Pattern 1: Optimistic Locking
// =============================================================================
// Assume no conflict, check version at commit time
// If version changed, retry the operation

#[derive(Clone)]
struct VersionedValue {
    data: String,
    version: u64,
}

struct OptimisticStore {
    data: DashMap<String, VersionedValue>,
    conflicts: AtomicU64,
    successes: AtomicU64,
}

impl OptimisticStore {
    fn new() -> Self {
        Self {
            data: DashMap::new(),
            conflicts: AtomicU64::new(0),
            successes: AtomicU64::new(0),
        }
    }

    fn read(&self, key: &str) -> Option<VersionedValue> {
        self.data.get(key).map(|v| v.clone())
    }

    /// Try to update with optimistic locking
    /// Returns Ok if successful, Err if version conflict
    fn compare_and_update(
        &self,
        key: &str,
        expected_version: u64,
        new_value: String,
    ) -> Result<u64, &'static str> {
        // Atomic check-and-update
        let mut entry = self.data.entry(key.to_string()).or_insert(VersionedValue {
            data: String::new(),
            version: 0,
        });

        if entry.version != expected_version {
            self.conflicts.fetch_add(1, Ordering::SeqCst);
            return Err("Version conflict - retry needed");
        }

        entry.version += 1;
        entry.data = new_value;
        self.successes.fetch_add(1, Ordering::SeqCst);
        Ok(entry.version)
    }

    /// Update with automatic retry
    fn update_with_retry<F>(&self, key: &str, max_retries: usize, transform: F) -> Result<u64, &'static str>
    where
        F: Fn(&str) -> String,
    {
        for _ in 0..max_retries {
            let current = self.read(key);
            let (current_data, version) = match current {
                Some(v) => (v.data, v.version),
                None => (String::new(), 0),
            };

            let new_data = transform(&current_data);

            match self.compare_and_update(key, version, new_data) {
                Ok(v) => return Ok(v),
                Err(_) => continue, // Retry
            }
        }
        Err("Max retries exceeded")
    }

    fn stats(&self) -> (u64, u64) {
        (
            self.successes.load(Ordering::SeqCst),
            self.conflicts.load(Ordering::SeqCst),
        )
    }
}

// =============================================================================
// Pattern 2: Pessimistic Locking
// =============================================================================
// Lock first, then work

struct PessimisticStore {
    data: HashMap<String, String>,
    locks: DashMap<String, ()>, // Track which keys are locked
    lock_waits: AtomicU64,
}

impl PessimisticStore {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
            locks: DashMap::new(),
            lock_waits: AtomicU64::new(0),
        }
    }

    /// Acquire lock for a key (simulated with DashMap entry)
    fn lock(&self, key: &str) -> LockGuard<'_> {
        // In real Redis: SET lock:key value NX EX 30
        while self.locks.contains_key(key) {
            self.lock_waits.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_micros(100));
        }
        self.locks.insert(key.to_string(), ());
        LockGuard {
            store: self,
            key: key.to_string(),
        }
    }
}

struct LockGuard<'a> {
    store: &'a PessimisticStore,
    key: String,
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        self.store.locks.remove(&self.key);
    }
}

// =============================================================================
// Pattern 3: Atomic Operations
// =============================================================================
// Use atomic CPU instructions for lock-free updates

struct AtomicCounter {
    counters: DashMap<String, AtomicU64>,
}

impl AtomicCounter {
    fn new() -> Self {
        Self {
            counters: DashMap::new(),
        }
    }

    fn increment(&self, key: &str) -> u64 {
        self.counters
            .entry(key.to_string())
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }

    fn decrement_if_positive(&self, key: &str) -> Result<u64, &'static str> {
        let entry = self
            .counters
            .entry(key.to_string())
            .or_insert(AtomicU64::new(0));

        // Compare-and-swap loop
        loop {
            let current = entry.load(Ordering::SeqCst);
            if current == 0 {
                return Err("Cannot decrement below zero");
            }

            // Atomic compare-and-swap
            match entry.compare_exchange(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Ok(current - 1),
                Err(_) => continue, // Value changed, retry
            }
        }
    }

    fn get(&self, key: &str) -> u64 {
        self.counters
            .get(key)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

// =============================================================================
// Pattern 4: Queue-Based Serialization
// =============================================================================
// Eliminate contention by processing requests sequentially

struct SerializedProcessor {
    queues: DashMap<String, Mutex<VecDeque<Box<dyn FnOnce(&mut i64) + Send>>>>,
    values: DashMap<String, Mutex<i64>>,
    processed: AtomicU64,
}

impl SerializedProcessor {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            queues: DashMap::new(),
            values: DashMap::new(),
            processed: AtomicU64::new(0),
        })
    }

    fn submit<F>(&self, key: &str, operation: F)
    where
        F: FnOnce(&mut i64) + Send + 'static,
    {
        self.queues
            .entry(key.to_string())
            .or_insert_with(|| Mutex::new(VecDeque::new()))
            .lock()
            .push_back(Box::new(operation));
    }

    fn process(&self, key: &str) {
        let queue_entry = self.queues.get(key);
        if queue_entry.is_none() {
            return;
        }

        let value_entry = self
            .values
            .entry(key.to_string())
            .or_insert(Mutex::new(0));
        let mut value = value_entry.lock();

        // Process all queued operations
        if let Some(queue_ref) = self.queues.get(key) {
            let mut queue = queue_ref.lock();
            while let Some(op) = queue.pop_front() {
                op(&mut *value);
                self.processed.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn get(&self, key: &str) -> i64 {
        self.values
            .get(key)
            .map(|v| *v.lock())
            .unwrap_or(0)
    }
}

// =============================================================================
// Pattern 5: Hot Key Mitigation
// =============================================================================
// Distribute hot key across multiple shards

struct HotKeyMitigator {
    shards: Vec<AtomicU64>,
    num_shards: usize,
    access_count: AtomicU64,
}

impl HotKeyMitigator {
    fn new(num_shards: usize) -> Self {
        Self {
            shards: (0..num_shards).map(|_| AtomicU64::new(0)).collect(),
            num_shards,
            access_count: AtomicU64::new(0),
        }
    }

    /// Increment total by adding to random shard
    fn increment(&self) -> u64 {
        self.access_count.fetch_add(1, Ordering::SeqCst);
        let shard_idx = rand::thread_rng().gen_range(0..self.num_shards);
        self.shards[shard_idx].fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get total by summing all shards
    fn get_total(&self) -> u64 {
        self.shards.iter().map(|s| s.load(Ordering::SeqCst)).sum()
    }

    /// Get approximate (faster, sample one shard and multiply)
    fn get_approximate(&self) -> u64 {
        let shard_idx = rand::thread_rng().gen_range(0..self.num_shards);
        self.shards[shard_idx].load(Ordering::SeqCst) * self.num_shards as u64
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("=== Handling Contention Pattern Demos ===\n");

    // Demo 1: Optimistic Locking
    println!("\n  ═══ Pattern 1: Optimistic Locking ═══");
    let store = Arc::new(OptimisticStore::new());

    // Initialize
    store.compare_and_update("balance", 0, "100".to_string()).unwrap();

    // Simulate concurrent updates
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let store = Arc::clone(&store);
            thread::spawn(move || {
                for _ in 0..10 {
                    let result = store.update_with_retry("balance", 5, |current| {
                        let balance: i64 = current.parse().unwrap_or(0);
                        (balance + 1).to_string()
                    });
                    if result.is_err() {
                        println!("Thread {} failed after retries", i);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let final_value = store.read("balance").unwrap();
    let (successes, conflicts) = store.stats();
    println!("Final balance: {}", final_value.data);
    println!(
        "Successes: {}, Conflicts: {}, Conflict rate: {:.1}%",
        successes,
        conflicts,
        conflicts as f64 / (successes + conflicts) as f64 * 100.0
    );
    println!();

    // Demo 2: Atomic Operations
    println!("\n  ═══ Pattern 2: Atomic Operations ═══");
    let counter = Arc::new(AtomicCounter::new());

    // Initialize inventory
    for _ in 0..100 {
        counter.increment("inventory:item1");
    }
    println!("Initial inventory: {}", counter.get("inventory:item1"));

    // Concurrent decrements (simulating purchases)
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..10 {
                    let _ = counter.decrement_if_positive("inventory:item1");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    println!("Final inventory: {}", counter.get("inventory:item1"));
    println!("No overselling occurred!\n");

    // Demo 3: Queue-Based Serialization
    println!("\n  ═══ Pattern 3: Queue-Based Serialization ═══");
    let processor = SerializedProcessor::new();

    // Submit operations
    for i in 0..50 {
        let delta = if i % 2 == 0 { 10 } else { -5 };
        processor.submit("account:1", move |balance| {
            *balance += delta;
        });
    }

    // Process queue
    processor.process("account:1");
    println!(
        "Final balance after 25 +10 and 25 -5 operations: {}",
        processor.get("account:1")
    );
    println!("(Expected: 25*10 - 25*5 = 125)\n");

    // Demo 4: Hot Key Mitigation
    println!("\n  ═══ Pattern 4: Hot Key Mitigation ═══");
    let mitigator = Arc::new(HotKeyMitigator::new(8)); // 8 shards

    // Simulate high concurrent access to hot key
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let m = Arc::clone(&mitigator);
            thread::spawn(move || {
                for _ in 0..1000 {
                    m.increment();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    println!("Total increments: {}", mitigator.get_total());
    println!("Approximate (fast): {}", mitigator.get_approximate());
    println!();

    // Demo 5: Compare conflict rates
    println!("\n  ═══ Comparison: Conflict Rates ═══");

    // High contention scenario
    let store_high = Arc::new(OptimisticStore::new());
    store_high.compare_and_update("hot", 0, "0".to_string()).unwrap();

    let handles: Vec<_> = (0..50)
        .map(|_| {
            let s = Arc::clone(&store_high);
            thread::spawn(move || {
                for _ in 0..20 {
                    let _ = s.update_with_retry("hot", 10, |v| {
                        let n: i64 = v.parse().unwrap_or(0);
                        (n + 1).to_string()
                    });
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let (s, c) = store_high.stats();
    println!("High contention (50 threads, 1 key):");
    println!("  Conflicts: {}, Conflict rate: {:.1}%", c, c as f64 / (s + c) as f64 * 100.0);

    println!("\n=== Key Takeaways ===");
    println!("1. Optimistic: Retry on conflict (good for low contention)");
    println!("2. Pessimistic: Block while locked (good for high contention)");
    println!("3. Atomic ops: Lock-free, CPU-level guarantees");
    println!("4. Queue-based: Serialize access, no contention by design");
    println!("5. Hot key: Shard the key to distribute load");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimistic_locking() {
        let store = OptimisticStore::new();

        // Initial write
        assert!(store.compare_and_update("key", 0, "v1".to_string()).is_ok());

        // Conflict on wrong version
        assert!(store.compare_and_update("key", 0, "v2".to_string()).is_err());

        // Success with correct version
        assert!(store.compare_and_update("key", 1, "v2".to_string()).is_ok());
    }

    #[test]
    fn test_atomic_no_oversell() {
        let counter = AtomicCounter::new();

        // Add 10 items
        for _ in 0..10 {
            counter.increment("item");
        }

        // Try to remove 15 (should fail 5 times)
        let mut succeeded = 0;
        for _ in 0..15 {
            if counter.decrement_if_positive("item").is_ok() {
                succeeded += 1;
            }
        }

        assert_eq!(succeeded, 10);
        assert_eq!(counter.get("item"), 0);
    }

    #[test]
    fn test_hot_key_sharding() {
        let m = HotKeyMitigator::new(4);

        for _ in 0..100 {
            m.increment();
        }

        assert_eq!(m.get_total(), 100);
    }
}
