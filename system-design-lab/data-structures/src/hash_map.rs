#![allow(dead_code, unused_variables, unused_imports)]
//! # Hash Map
//!
//! Two implementations:
//! 1. Separate chaining (linked list per bucket)
//! 2. Open addressing with linear probing

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

// =============================================================================
// Separate Chaining HashMap
// =============================================================================

const INITIAL_BUCKETS: usize = 16;
const LOAD_FACTOR_THRESHOLD: f64 = 0.75;

struct Entry<K, V> {
    key: K,
    value: V,
}

pub struct ChainingHashMap<K, V> {
    buckets: Vec<Vec<Entry<K, V>>>,
    len: usize,
}

impl<K: Hash + Eq, V> ChainingHashMap<K, V> {
    pub fn new() -> Self {
        let mut buckets = Vec::with_capacity(INITIAL_BUCKETS);
        for _ in 0..INITIAL_BUCKETS {
            buckets.push(Vec::new());
        }
        Self { buckets, len: 0 }
    }

    fn hash(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.buckets.len()
    }

    fn load_factor(&self) -> f64 {
        self.len as f64 / self.buckets.len() as f64
    }

    fn resize(&mut self) {
        let new_cap = self.buckets.len() * 2;
        let mut new_buckets = Vec::with_capacity(new_cap);
        for _ in 0..new_cap {
            new_buckets.push(Vec::new());
        }

        let old_buckets = std::mem::replace(&mut self.buckets, new_buckets);
        self.len = 0;

        for bucket in old_buckets {
            for entry in bucket {
                self.insert(entry.key, entry.value);
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.load_factor() > LOAD_FACTOR_THRESHOLD {
            self.resize();
        }

        let idx = self.hash(&key);
        // Check if key exists and update
        for entry in &mut self.buckets[idx] {
            if entry.key == key {
                let old = std::mem::replace(&mut entry.value, value);
                return Some(old);
            }
        }
        // Insert new
        self.buckets[idx].push(Entry { key, value });
        self.len += 1;
        None
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let idx = self.hash(key);
        self.buckets[idx]
            .iter()
            .find(|e| &e.key == key)
            .map(|e| &e.value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let idx = self.hash(key);
        let pos = self.buckets[idx].iter().position(|e| &e.key == key)?;
        self.len -= 1;
        Some(self.buckets[idx].swap_remove(pos).value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

// =============================================================================
// Open Addressing HashMap (Linear Probing)
// =============================================================================

enum Slot<K, V> {
    Empty,
    Occupied(K, V),
    Tombstone,
}

pub struct ProbingHashMap<K, V> {
    slots: Vec<Slot<K, V>>,
    len: usize,
    cap: usize,
}

impl<K: Hash + Eq, V> ProbingHashMap<K, V> {
    pub fn new() -> Self {
        let cap = INITIAL_BUCKETS;
        let mut slots = Vec::with_capacity(cap);
        for _ in 0..cap {
            slots.push(Slot::Empty);
        }
        Self { slots, len: 0, cap }
    }

    fn hash(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.cap
    }

    fn resize(&mut self) {
        let new_cap = self.cap * 2;
        let mut new_slots = Vec::with_capacity(new_cap);
        for _ in 0..new_cap {
            new_slots.push(Slot::Empty);
        }
        let old_slots = std::mem::replace(&mut self.slots, new_slots);
        let old_cap = self.cap;
        self.cap = new_cap;
        self.len = 0;

        for slot in old_slots {
            if let Slot::Occupied(k, v) = slot {
                self.insert(k, v);
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if (self.len + 1) as f64 / self.cap as f64 > LOAD_FACTOR_THRESHOLD {
            self.resize();
        }

        let mut idx = self.hash(&key);
        let mut tombstone_idx = None;

        loop {
            match &self.slots[idx] {
                Slot::Occupied(k, _) if *k == key => {
                    // Update existing
                    let old = std::mem::replace(&mut self.slots[idx], Slot::Occupied(key, value));
                    return match old {
                        Slot::Occupied(_, v) => Some(v),
                        _ => unreachable!(),
                    };
                }
                Slot::Empty => {
                    let target = tombstone_idx.unwrap_or(idx);
                    self.slots[target] = Slot::Occupied(key, value);
                    self.len += 1;
                    return None;
                }
                Slot::Tombstone => {
                    if tombstone_idx.is_none() {
                        tombstone_idx = Some(idx);
                    }
                }
                _ => {}
            }
            idx = (idx + 1) % self.cap;
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut idx = self.hash(key);
        loop {
            match &self.slots[idx] {
                Slot::Occupied(k, v) if k == key => return Some(v),
                Slot::Empty => return None,
                _ => idx = (idx + 1) % self.cap,
            }
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let mut idx = self.hash(key);
        loop {
            match &self.slots[idx] {
                Slot::Occupied(k, _) if k == key => {
                    let old = std::mem::replace(&mut self.slots[idx], Slot::Tombstone);
                    self.len -= 1;
                    return match old {
                        Slot::Occupied(_, v) => Some(v),
                        _ => unreachable!(),
                    };
                }
                Slot::Empty => return None,
                _ => idx = (idx + 1) % self.cap,
            }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Chaining HashMap ===");
    let mut map = ChainingHashMap::new();
    map.insert("apple", 3);
    map.insert("banana", 5);
    map.insert("cherry", 7);
    println!("apple => {:?}", map.get(&"apple"));
    println!("banana => {:?}", map.get(&"banana"));
    map.insert("apple", 10);
    println!("apple (updated) => {:?}", map.get(&"apple"));
    map.remove(&"banana");
    println!("banana (removed) => {:?}", map.get(&"banana"));
    println!("len = {}", map.len());

    // Test rehashing with many inserts
    let mut big_map = ChainingHashMap::new();
    for i in 0..100 {
        big_map.insert(i, i * i);
    }
    println!("big_map[42] = {:?}", big_map.get(&42));
    println!("big_map len = {}", big_map.len());

    println!("\n=== Probing HashMap ===");
    let mut pmap = ProbingHashMap::new();
    pmap.insert("x", 1);
    pmap.insert("y", 2);
    pmap.insert("z", 3);
    println!("x => {:?}", pmap.get(&"x"));
    pmap.remove(&"y");
    println!("z (after removing y) => {:?}", pmap.get(&"z"));
    println!("len = {}", pmap.len());
}
