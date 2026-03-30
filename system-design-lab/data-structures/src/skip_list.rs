#![allow(dead_code, unused_variables, unused_imports)]
//! # Skip List
//!
//! A probabilistic sorted data structure with O(log n) average search/insert/delete.
//! Alternative to balanced BSTs — simpler to implement and reason about.
//!
//! Structure: Multiple layers of linked lists.
//! - Bottom layer (level 0): all elements in sorted order.
//! - Higher layers: "express lanes" skipping elements.
//! - Each element promoted to next level with probability p (typically 0.5).

use rand::Rng;

const MAX_LEVEL: usize = 16;
const P: f64 = 0.5; // promotion probability

struct SkipNode<K: Ord, V> {
    key: K,
    value: V,
    forward: Vec<Option<usize>>, // forward[i] = next node at level i
}

/// A sentinel-headed skip list.
pub struct SkipList<K: Ord, V> {
    arena: Vec<SkipNode<K, V>>,
    head: usize,       // sentinel node index
    level: usize,      // current max level in use
    len: usize,
}

impl<K: Ord + Default + Clone + std::fmt::Debug, V: Default + Clone> SkipList<K, V> {
    pub fn new() -> Self {
        let sentinel = SkipNode {
            key: K::default(),
            value: V::default(),
            forward: vec![None; MAX_LEVEL],
        };
        Self {
            arena: vec![sentinel],
            head: 0,
            level: 0,
            len: 0,
        }
    }

    fn random_level(&self) -> usize {
        let mut rng = rand::thread_rng();
        let mut lvl = 0;
        while lvl < MAX_LEVEL - 1 && rng.gen::<f64>() < P {
            lvl += 1;
        }
        lvl
    }

    pub fn insert(&mut self, key: K, value: V) {
        // Find update positions at each level
        let mut update = vec![self.head; MAX_LEVEL];
        let mut current = self.head;

        for lvl in (0..=self.level).rev() {
            while let Some(next_idx) = self.arena[current].forward[lvl] {
                if self.arena[next_idx].key < key {
                    current = next_idx;
                } else {
                    break;
                }
            }
            update[lvl] = current;
        }

        // Check if key already exists at level 0
        if let Some(next_idx) = self.arena[current].forward[0] {
            if self.arena[next_idx].key == key {
                self.arena[next_idx].value = value;
                return;
            }
        }

        let new_level = self.random_level();
        if new_level > self.level {
            for lvl in (self.level + 1)..=new_level {
                update[lvl] = self.head;
            }
            self.level = new_level;
        }

        let new_idx = self.arena.len();
        self.arena.push(SkipNode {
            key,
            value,
            forward: vec![None; new_level + 1],
        });

        for lvl in 0..=new_level {
            self.arena[new_idx].forward[lvl] = self.arena[update[lvl]].forward[lvl];
            self.arena[update[lvl]].forward[lvl] = Some(new_idx);
        }

        self.len += 1;
    }

    pub fn search(&self, key: &K) -> Option<&V> {
        let mut current = self.head;
        for lvl in (0..=self.level).rev() {
            while let Some(next_idx) = self.arena[current].forward[lvl] {
                if &self.arena[next_idx].key < key {
                    current = next_idx;
                } else if &self.arena[next_idx].key == key {
                    return Some(&self.arena[next_idx].value);
                } else {
                    break;
                }
            }
        }
        None
    }

    pub fn contains(&self, key: &K) -> bool {
        self.search(key).is_some()
    }

    /// Returns sorted key-value pairs.
    pub fn to_sorted_vec(&self) -> Vec<(&K, &V)> {
        let mut result = Vec::new();
        let mut current = self.arena[self.head].forward[0];
        while let Some(idx) = current {
            result.push((&self.arena[idx].key, &self.arena[idx].value));
            current = self.arena[idx].forward[0];
        }
        result
    }

    /// Range query: all entries with key in [low, high].
    pub fn range(&self, low: &K, high: &K) -> Vec<(&K, &V)> {
        let mut result = Vec::new();
        // Find first node >= low
        let mut current = self.head;
        for lvl in (0..=self.level).rev() {
            while let Some(next_idx) = self.arena[current].forward[lvl] {
                if &self.arena[next_idx].key < low {
                    current = next_idx;
                } else {
                    break;
                }
            }
        }
        // Walk level 0 collecting entries
        current = self.arena[current].forward[0].unwrap_or(self.head);
        while current != self.head {
            if &self.arena[current].key > high {
                break;
            }
            if &self.arena[current].key >= low {
                result.push((&self.arena[current].key, &self.arena[current].value));
            }
            current = self.arena[current].forward[0].unwrap_or(self.head);
            if self.arena[current].forward[0].is_none() && current != self.head {
                // Last element
                if &self.arena[current].key >= low && &self.arena[current].key <= high {
                    result.push((&self.arena[current].key, &self.arena[current].value));
                }
                break;
            }
        }
        result
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn current_level(&self) -> usize {
        self.level
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Skip List ===");
    let mut sl = SkipList::new();

    // Insert in random order
    let keys = [5, 3, 8, 1, 9, 4, 7, 2, 6, 10];
    for &k in &keys {
        sl.insert(k, k * 100);
    }

    println!("Inserted: {keys:?}");
    println!("Sorted: {:?}", sl.to_sorted_vec());
    println!("Search 7: {:?}", sl.search(&7));
    println!("Search 11: {:?}", sl.search(&11));
    println!("Contains 5: {}", sl.contains(&5));
    println!("Max level in use: {}", sl.current_level());
    println!("Len: {}", sl.len());

    // Update existing key
    sl.insert(5, 555);
    println!("After update, search 5: {:?}", sl.search(&5));
}
