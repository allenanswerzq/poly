//! # Caching Implementations
//!
//! This module demonstrates various cache implementations:
//! 1. LRU Cache - Using doubly-linked list + HashMap
//! 2. TTL LRU Cache - LRU with time-based expiration
//! 3. Concurrent Cache - Thread-safe LRU using parking_lot

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

// =============================================================================
// LRU Cache Implementation
// =============================================================================

/// A node in the doubly-linked list
struct LruNode<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
}

/// LRU Cache using a HashMap + doubly-linked list
///
/// Operations:
/// - get: O(1)
/// - put: O(1)
/// - eviction: O(1)
pub struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, usize>,  // key -> index in nodes
    nodes: Vec<Option<LruNode<K, V>>>,
    head: Option<usize>,     // Most recently used
    tail: Option<usize>,     // Least recently used
    free_slots: Vec<usize>,  // Reusable indices
}

impl<K: Clone + Hash + Eq, V: Clone> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::with_capacity(capacity),
            nodes: Vec::with_capacity(capacity),
            head: None,
            tail: None,
            free_slots: Vec::new(),
        }
    }

    /// Get a value, moving it to the front (most recently used)
    pub fn get(&mut self, key: &K) -> Option<V> {
        if let Some(&idx) = self.map.get(key) {
            // Move to front
            self.move_to_front(idx);
            // Return value
            self.nodes[idx].as_ref().map(|n| n.value.clone())
        } else {
            None
        }
    }

    /// Insert or update a key-value pair
    pub fn put(&mut self, key: K, value: V) {
        if let Some(&idx) = self.map.get(&key) {
            // Update existing
            if let Some(node) = &mut self.nodes[idx] {
                node.value = value;
            }
            self.move_to_front(idx);
        } else {
            // Evict if necessary
            if self.map.len() >= self.capacity {
                self.evict_lru();
            }

            // Insert new node
            let idx = self.allocate_slot();
            let node = LruNode {
                key: key.clone(),
                value,
                prev: None,
                next: self.head,
            };

            if idx >= self.nodes.len() {
                self.nodes.push(Some(node));
            } else {
                self.nodes[idx] = Some(node);
            }

            // Update head's prev pointer
            if let Some(head_idx) = self.head {
                if let Some(head_node) = &mut self.nodes[head_idx] {
                    head_node.prev = Some(idx);
                }
            }

            self.head = Some(idx);
            if self.tail.is_none() {
                self.tail = Some(idx);
            }

            self.map.insert(key, idx);
        }
    }

    /// Remove the least recently used item
    fn evict_lru(&mut self) {
        if let Some(tail_idx) = self.tail {
            if let Some(tail_node) = self.nodes[tail_idx].take() {
                // Update tail pointer
                self.tail = tail_node.prev;
                if let Some(new_tail) = self.tail {
                    if let Some(node) = &mut self.nodes[new_tail] {
                        node.next = None;
                    }
                } else {
                    self.head = None;
                }

                // Remove from map and mark slot as free
                self.map.remove(&tail_node.key);
                self.free_slots.push(tail_idx);
            }
        }
    }

    /// Move a node to the front of the list
    fn move_to_front(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return; // Already at front
        }

        // Get the node's neighbors
        let (prev, next) = {
            let node = self.nodes[idx].as_ref().unwrap();
            (node.prev, node.next)
        };

        // Remove from current position
        if let Some(prev_idx) = prev {
            if let Some(prev_node) = &mut self.nodes[prev_idx] {
                prev_node.next = next;
            }
        }
        if let Some(next_idx) = next {
            if let Some(next_node) = &mut self.nodes[next_idx] {
                next_node.prev = prev;
            }
        }

        // Update tail if necessary
        if self.tail == Some(idx) {
            self.tail = prev;
        }

        // Move to front
        if let Some(node) = &mut self.nodes[idx] {
            node.prev = None;
            node.next = self.head;
        }
        if let Some(head_idx) = self.head {
            if let Some(head_node) = &mut self.nodes[head_idx] {
                head_node.prev = Some(idx);
            }
        }
        self.head = Some(idx);
    }

    fn allocate_slot(&mut self) -> usize {
        self.free_slots.pop().unwrap_or(self.nodes.len())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// =============================================================================
// TTL LRU Cache Implementation
// =============================================================================

#[derive(Clone)]
struct TtlEntry<V: Clone> {
    value: V,
    expires_at: Instant,
}

/// LRU Cache with Time-To-Live expiration
pub struct TtlLruCache<K, V: Clone> {
    inner: LruCache<K, TtlEntry<V>>,
    default_ttl: Duration,
}

impl<K: Clone + Hash + Eq, V: Clone> TtlLruCache<K, V> {
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            inner: LruCache::new(capacity),
            default_ttl: ttl,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        if let Some(entry) = self.inner.get(key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.value.clone());
            }
            // Expired - could remove here, but LRU will handle it
        }
        None
    }

    pub fn put(&mut self, key: K, value: V) {
        self.put_with_ttl(key, value, self.default_ttl);
    }

    pub fn put_with_ttl(&mut self, key: K, value: V, ttl: Duration) {
        let entry = TtlEntry {
            value,
            expires_at: Instant::now() + ttl,
        };
        self.inner.put(key, entry);
    }
}

// =============================================================================
// Concurrent LRU Cache (Thread-Safe)
// =============================================================================

use dashmap::DashMap;

/// A thread-safe approximate LRU cache using sharded maps
///
/// This trades exact LRU ordering for better concurrent performance.
/// Good for high-throughput scenarios where approximate LRU is acceptable.
pub struct ConcurrentCache<K, V> {
    data: DashMap<K, (V, Instant)>,
    capacity: usize,
}

impl<K: Clone + Hash + Eq, V: Clone> ConcurrentCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: DashMap::with_capacity(capacity),
            capacity,
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.data.get(key).map(|entry| {
            // In a more sophisticated impl, we'd update access time
            entry.value().0.clone()
        })
    }

    pub fn put(&self, key: K, value: V) {
        // Simple eviction: if over capacity, remove oldest entries
        if self.data.len() >= self.capacity {
            self.evict_oldest();
        }
        self.data.insert(key, (value, Instant::now()));
    }

    fn evict_oldest(&self) {
        // Find and remove the oldest entry
        let mut oldest_key = None;
        let mut oldest_time = Instant::now();

        for entry in self.data.iter() {
            if entry.value().1 < oldest_time {
                oldest_time = entry.value().1;
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.data.remove(&key);
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Caching Implementations Demo ===\n");

    // Demo 1: Basic LRU Cache
    println!("\n  ═══ LRU Cache ═══");
    let mut lru = LruCache::new(3);

    lru.put("a", 1);
    lru.put("b", 2);
    lru.put("c", 3);
    println!("Added a=1, b=2, c=3");

    println!("Get a: {:?}", lru.get(&"a"));  // Access 'a', moves to front

    lru.put("d", 4);  // This should evict 'b' (LRU)
    println!("Added d=4 (should evict b)");

    println!("Get b: {:?}", lru.get(&"b"));  // Should be None
    println!("Get c: {:?}", lru.get(&"c"));  // Should be Some(3)
    println!("Get d: {:?}", lru.get(&"d"));  // Should be Some(4)
    println!("Cache size: {}\n", lru.len());

    // Demo 2: TTL LRU Cache
    println!("\n  ═══ TTL LRU Cache ═══");
    let mut ttl_cache = TtlLruCache::new(10, Duration::from_millis(100));

    ttl_cache.put("session", "user123");
    println!("Stored session (TTL=100ms)");
    println!("Get session: {:?}", ttl_cache.get(&"session"));

    println!("Sleeping 150ms...");
    std::thread::sleep(Duration::from_millis(150));

    println!("Get session after TTL: {:?}", ttl_cache.get(&"session"));  // Expired

    // Store with custom TTL
    ttl_cache.put_with_ttl("long_lived", "data", Duration::from_secs(60));
    println!("Stored long_lived with 60s TTL\n");

    // Demo 3: Concurrent Cache
    println!("\n  ═══ Concurrent Cache ═══");
    let cache = ConcurrentCache::new(1000);

    // Simulate concurrent access
    use std::thread;
    use std::sync::Arc;

    let cache = Arc::new(cache);
    let mut handles = vec![];

    // Writer threads
    for i in 0..4 {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for j in 0..100 {
                cache.put(format!("key-{}-{}", i, j), j);
            }
        }));
    }

    // Reader threads
    for i in 0..4 {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for j in 0..100 {
                let _ = cache.get(&format!("key-{}-{}", i, j));
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("After concurrent operations: {} entries", cache.len());

    // Demo 4: Cache-aside pattern simulation
    println!("\n--- Cache-Aside Pattern Demo ---");
    demo_cache_aside();
}

/// Demonstrates the cache-aside pattern
fn demo_cache_aside() {
    // Simulated "database"
    let database: HashMap<&str, i32> = [
        ("user:1", 100),
        ("user:2", 200),
        ("user:3", 300),
    ].into_iter().collect();

    let mut cache: LruCache<&str, i32> = LruCache::new(10);
    let mut cache_hits = 0;
    let mut cache_misses = 0;

    // Simulate requests
    let requests = ["user:1", "user:2", "user:1", "user:1", "user:3", "user:2", "user:1"];

    for key in requests {
        if let Some(value) = cache.get(&key) {
            println!("  Cache HIT: {} = {}", key, value);
            cache_hits += 1;
        } else {
            println!("  Cache MISS: {} - fetching from DB", key);
            cache_misses += 1;

            // Fetch from "database"
            if let Some(&value) = database.get(key) {
                cache.put(key, value);
                println!("    Loaded {} = {} into cache", key, value);
            }
        }
    }

    let hit_rate = cache_hits as f64 / (cache_hits + cache_misses) as f64 * 100.0;
    println!("\nCache stats: {} hits, {} misses ({:.1}% hit rate)",
             cache_hits, cache_misses, hit_rate);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_basic() {
        let mut cache = LruCache::new(2);
        cache.put("a", 1);
        cache.put("b", 2);

        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = LruCache::new(2);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);  // Should evict 'a'

        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn test_lru_access_updates_order() {
        let mut cache = LruCache::new(2);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.get(&"a");     // Access 'a', making 'b' the LRU
        cache.put("c", 3);   // Should evict 'b'

        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), None);
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn test_ttl_expiration() {
        let mut cache = TtlLruCache::new(10, Duration::from_millis(50));
        cache.put("key", "value");

        assert_eq!(cache.get(&"key"), Some("value"));

        std::thread::sleep(Duration::from_millis(60));

        assert_eq!(cache.get(&"key"), None);
    }
}
