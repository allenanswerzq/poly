#![allow(dead_code, unused_variables, unused_imports)]
//! # LRU Cache & LFU Cache
//!
//! - LRU: O(1) get/put using HashMap + doubly linked list
//! - LFU: O(1) get/put using HashMap + frequency buckets

use std::collections::HashMap;

// =============================================================================
// LRU Cache — O(1) get and put
// =============================================================================
// HashMap<key -> (node_idx, value)> + DoublyLinkedList of keys for ordering.
// Most recently used at front, least recently used at back.

struct LruNode {
    key: i32,
    prev: Option<usize>,
    next: Option<usize>,
}

pub struct LruCache {
    capacity: usize,
    map: HashMap<i32, (usize, i32)>, // key -> (node_idx, value)
    nodes: Vec<LruNode>,
    head: Option<usize>, // most recently used
    tail: Option<usize>, // least recently used
    free: Vec<usize>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            nodes: Vec::new(),
            head: None,
            tail: None,
            free: Vec::new(),
        }
    }

    pub fn get(&mut self, key: i32) -> Option<i32> {
        let &(node_idx, value) = self.map.get(&key)?;
        self.move_to_front(node_idx);
        Some(value)
    }

    pub fn put(&mut self, key: i32, value: i32) {
        if let Some(&(node_idx, _)) = self.map.get(&key) {
            self.map.insert(key, (node_idx, value));
            self.move_to_front(node_idx);
            return;
        }

        // Evict if at capacity
        if self.map.len() >= self.capacity {
            if let Some(tail_idx) = self.tail {
                let evicted_key = self.nodes[tail_idx].key;
                self.remove_node(tail_idx);
                self.map.remove(&evicted_key);
                self.free.push(tail_idx);
            }
        }

        // Allocate new node
        let node_idx = if let Some(idx) = self.free.pop() {
            self.nodes[idx] = LruNode {
                key,
                prev: None,
                next: None,
            };
            idx
        } else {
            let idx = self.nodes.len();
            self.nodes.push(LruNode {
                key,
                prev: None,
                next: None,
            });
            idx
        };

        self.push_front(node_idx);
        self.map.insert(key, (node_idx, value));
    }

    fn push_front(&mut self, idx: usize) {
        self.nodes[idx].prev = None;
        self.nodes[idx].next = self.head;
        if let Some(old_head) = self.head {
            self.nodes[old_head].prev = Some(idx);
        }
        self.head = Some(idx);
        if self.tail.is_none() {
            self.tail = Some(idx);
        }
    }

    fn remove_node(&mut self, idx: usize) {
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;
        if let Some(p) = prev {
            self.nodes[p].next = next;
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            self.nodes[n].prev = prev;
        } else {
            self.tail = prev;
        }
    }

    fn move_to_front(&mut self, idx: usize) {
        if self.head == Some(idx) {
            return;
        }
        self.remove_node(idx);
        self.push_front(idx);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

// =============================================================================
// LFU Cache — O(1) get and put
// =============================================================================
// Uses three HashMaps:
// 1. key -> (value, frequency)
// 2. frequency -> ordered set of keys (as a Vec for simplicity, LinkedHashSet ideally)
// 3. Track min frequency
//
// On get: increment frequency, move key to new frequency bucket.
// On put: evict from min-frequency bucket if full.

pub struct LfuCache {
    capacity: usize,
    min_freq: usize,
    key_val: HashMap<i32, (i32, usize)>,    // key -> (value, freq)
    freq_keys: HashMap<usize, Vec<i32>>,     // freq -> keys in insertion order
    key_pos: HashMap<i32, usize>,            // key -> position in its freq bucket
}

impl LfuCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            min_freq: 0,
            key_val: HashMap::new(),
            freq_keys: HashMap::new(),
            key_pos: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: i32) -> Option<i32> {
        let &(value, freq) = self.key_val.get(&key)?;
        self.touch(key, freq);
        Some(value)
    }

    pub fn put(&mut self, key: i32, value: i32) {
        if self.capacity == 0 {
            return;
        }

        // Update existing key
        if let Some(&(_, freq)) = self.key_val.get(&key) {
            self.key_val.insert(key, (value, freq));
            self.touch(key, freq);
            return;
        }

        // Evict if full
        if self.key_val.len() >= self.capacity {
            self.evict();
        }

        // Insert new key with freq 1
        self.key_val.insert(key, (value, 1));
        let bucket = self.freq_keys.entry(1).or_default();
        self.key_pos.insert(key, bucket.len());
        bucket.push(key);
        self.min_freq = 1;
    }

    fn touch(&mut self, key: i32, old_freq: usize) {
        // Remove from old frequency bucket
        self.remove_from_bucket(key, old_freq);

        // Add to new frequency bucket
        let new_freq = old_freq + 1;
        self.key_val.get_mut(&key).unwrap().1 = new_freq;
        let bucket = self.freq_keys.entry(new_freq).or_default();
        self.key_pos.insert(key, bucket.len());
        bucket.push(key);

        // Update min_freq if we emptied the min bucket
        if self.min_freq == old_freq {
            if self
                .freq_keys
                .get(&old_freq)
                .map_or(true, |b| b.is_empty())
            {
                self.min_freq = new_freq;
            }
        }
    }

    fn remove_from_bucket(&mut self, key: i32, freq: usize) {
        let bucket = self.freq_keys.get_mut(&freq).unwrap();
        let pos = self.key_pos[&key];
        bucket.swap_remove(pos);
        // Update swapped element's position
        if pos < bucket.len() {
            let swapped_key = bucket[pos];
            self.key_pos.insert(swapped_key, pos);
        }
        self.key_pos.remove(&key);
    }

    fn evict(&mut self) {
        let bucket = self.freq_keys.get_mut(&self.min_freq).unwrap();
        let evicted = bucket.remove(0); // oldest in min-freq bucket
        // Shift positions
        for (i, &k) in bucket.iter().enumerate() {
            self.key_pos.insert(k, i);
        }
        self.key_pos.remove(&evicted);
        self.key_val.remove(&evicted);
    }

    pub fn len(&self) -> usize {
        self.key_val.len()
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== LRU Cache ===");
    let mut lru = LruCache::new(3);
    lru.put(1, 10);
    lru.put(2, 20);
    lru.put(3, 30);
    println!("get(1) = {:?}", lru.get(1)); // 10, moves 1 to front
    lru.put(4, 40); // evicts 2 (LRU)
    println!("get(2) = {:?}", lru.get(2)); // None (evicted)
    println!("get(3) = {:?}", lru.get(3)); // 30
    println!("get(4) = {:?}", lru.get(4)); // 40
    lru.put(5, 50); // evicts 1
    println!("get(1) = {:?}", lru.get(1)); // None

    println!("\n=== LFU Cache ===");
    let mut lfu = LfuCache::new(3);
    lfu.put(1, 10);
    lfu.put(2, 20);
    lfu.put(3, 30);
    lfu.get(1); // freq: 1->2
    lfu.get(1); // freq: 2->3
    lfu.get(2); // freq: 1->2
    lfu.put(4, 40); // evicts 3 (least frequently used, freq=1)
    println!("get(3) = {:?}", lfu.get(3)); // None
    println!("get(1) = {:?}", lfu.get(1)); // 10
    println!("get(2) = {:?}", lfu.get(2)); // 20
    println!("get(4) = {:?}", lfu.get(4)); // 40
}
