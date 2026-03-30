#![allow(dead_code, unused_variables, unused_imports)]
//! # Thread-Safe Data Structures
//!
//! - Sharded concurrent HashMap (fine-grained locking)
//! - Bounded blocking queue (producer-consumer)
//! - RwLock-protected sorted set

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::{Mutex, RwLock, Condvar};

// =============================================================================
// Sharded Concurrent HashMap
// =============================================================================
// Instead of one lock for the whole map, we shard into N buckets each with
// its own RwLock. This reduces contention dramatically.

const NUM_SHARDS: usize = 16;

struct Shard<K, V> {
    data: RwLock<Vec<(K, V)>>,
}

pub struct ShardedMap<K, V> {
    shards: Vec<Shard<K, V>>,
}

impl<K: Hash + Eq + Clone, V: Clone> ShardedMap<K, V> {
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            shards.push(Shard {
                data: RwLock::new(Vec::new()),
            });
        }
        Self { shards }
    }

    fn shard_idx(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % NUM_SHARDS
    }

    pub fn insert(&self, key: K, value: V) {
        let idx = self.shard_idx(&key);
        let mut shard = self.shards[idx].data.write();
        if let Some(entry) = shard.iter_mut().find(|(k, _)| k == &key) {
            entry.1 = value;
        } else {
            shard.push((key, value));
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let idx = self.shard_idx(key);
        let shard = self.shards[idx].data.read();
        shard.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let idx = self.shard_idx(key);
        let mut shard = self.shards[idx].data.write();
        if let Some(pos) = shard.iter().position(|(k, _)| k == key) {
            Some(shard.swap_remove(pos).1)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.data.read().len()).sum()
    }
}

// Safe to share across threads
unsafe impl<K: Send, V: Send> Send for ShardedMap<K, V> {}
unsafe impl<K: Send + Sync, V: Send + Sync> Sync for ShardedMap<K, V> {}

// =============================================================================
// Bounded Blocking Queue (Producer-Consumer)
// =============================================================================
// Uses Mutex + Condvar for blocking when full/empty.

pub struct BlockingQueue<T> {
    data: Mutex<Vec<T>>,
    capacity: usize,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BlockingQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: Mutex::new(Vec::with_capacity(capacity)),
            capacity,
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        }
    }

    /// Blocking push — waits if queue is full.
    pub fn push(&self, value: T) {
        let mut data = self.data.lock();
        while data.len() >= self.capacity {
            self.not_full.wait(&mut data);
        }
        data.push(value);
        self.not_empty.notify_one();
    }

    /// Blocking pop — waits if queue is empty.
    pub fn pop(&self) -> T {
        let mut data = self.data.lock();
        while data.is_empty() {
            self.not_empty.wait(&mut data);
        }
        let value = data.remove(0);
        self.not_full.notify_one();
        value
    }

    /// Non-blocking try_push.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let mut data = self.data.lock();
        if data.len() >= self.capacity {
            Err(value)
        } else {
            data.push(value);
            self.not_empty.notify_one();
            Ok(())
        }
    }

    /// Non-blocking try_pop.
    pub fn try_pop(&self) -> Option<T> {
        let mut data = self.data.lock();
        if data.is_empty() {
            None
        } else {
            let value = data.remove(0);
            self.not_full.notify_one();
            Some(value)
        }
    }

    pub fn len(&self) -> usize {
        self.data.lock().len()
    }
}

// =============================================================================
// Thread-safe Sorted Set (RwLock<BTreeSet>)
// =============================================================================

pub struct ConcurrentSortedSet<T: Ord> {
    inner: RwLock<BTreeSet<T>>,
}

impl<T: Ord + Clone> ConcurrentSortedSet<T> {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(BTreeSet::new()),
        }
    }

    pub fn insert(&self, value: T) -> bool {
        self.inner.write().insert(value)
    }

    pub fn remove(&self, value: &T) -> bool {
        self.inner.write().remove(value)
    }

    pub fn contains(&self, value: &T) -> bool {
        self.inner.read().contains(value)
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// Range query: returns all values in [low, high].
    pub fn range(&self, low: &T, high: &T) -> Vec<T> {
        use std::ops::RangeInclusive;
        self.inner
            .read()
            .range(low.clone()..=high.clone())
            .cloned()
            .collect()
    }
}

// =============================================================================
// Read-Write Counter (demonstrates RwLock advantage)
// =============================================================================

pub struct RwCounter {
    inner: RwLock<i64>,
}

impl RwCounter {
    pub fn new(val: i64) -> Self {
        Self {
            inner: RwLock::new(val),
        }
    }

    /// Many readers can read concurrently.
    pub fn read(&self) -> i64 {
        *self.inner.read()
    }

    /// Writers get exclusive access.
    pub fn increment(&self) {
        *self.inner.write() += 1;
    }

    pub fn add(&self, n: i64) {
        *self.inner.write() += n;
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Sharded Concurrent HashMap ===");
    let map = Arc::new(ShardedMap::new());
    let mut handles = Vec::new();

    // Spawn 4 writer threads
    for t in 0..4 {
        let map = Arc::clone(&map);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                map.insert(t * 100 + i, i);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("Map size after 4 threads x 100 inserts: {}", map.len());
    println!("map[42] = {:?}", map.get(&42));

    println!("\n=== Blocking Queue (Producer-Consumer) ===");
    let queue = Arc::new(BlockingQueue::new(5));
    let q_producer = Arc::clone(&queue);
    let q_consumer = Arc::clone(&queue);

    let producer = thread::spawn(move || {
        for i in 0..10 {
            q_producer.push(i);
            println!("  produced: {i}");
        }
    });

    let consumer = thread::spawn(move || {
        for _ in 0..10 {
            let v = q_consumer.pop();
            println!("  consumed: {v}");
        }
    });

    producer.join().unwrap();
    consumer.join().unwrap();

    println!("\n=== Concurrent Sorted Set ===");
    let set = Arc::new(ConcurrentSortedSet::new());
    let mut handles = Vec::new();
    for t in 0..4 {
        let set = Arc::clone(&set);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                set.insert(t * 25 + i);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!("Set size: {}", set.len());
    println!("Range [10, 20]: {:?}", set.range(&10, &20));
}
