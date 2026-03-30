#![allow(dead_code, unused_variables, unused_imports)]
//! # Binary Heap & Priority Queue
//!
//! Array-backed binary min-heap with a generic priority queue wrapper.

use std::fmt;

// =============================================================================
// Binary Heap (Min-Heap)
// =============================================================================

pub struct MinHeap<T: Ord> {
    data: Vec<T>,
}

impl<T: Ord> MinHeap<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Build a heap from an unsorted Vec — O(n).
    pub fn from_vec(mut data: Vec<T>) -> Self {
        let n = data.len();
        // Heapify: sift down from the last parent to root
        if n > 0 {
            for i in (0..n / 2).rev() {
                Self::sift_down_slice(&mut data, i, n);
            }
        }
        Self { data }
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
        self.sift_up(self.data.len() - 1);
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }
        let last = self.data.len() - 1;
        self.data.swap(0, last);
        let min = self.data.pop();
        if !self.data.is_empty() {
            self.sift_down(0);
        }
        min
    }

    pub fn peek(&self) -> Option<&T> {
        self.data.first()
    }

    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx] < self.data[parent] {
                self.data.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, idx: usize) {
        let n = self.data.len();
        Self::sift_down_slice(&mut self.data, idx, n);
    }

    fn sift_down_slice(data: &mut [T], mut idx: usize, n: usize) {
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut smallest = idx;

            if left < n && data[left] < data[smallest] {
                smallest = left;
            }
            if right < n && data[right] < data[smallest] {
                smallest = right;
            }

            if smallest != idx {
                data.swap(idx, smallest);
                idx = smallest;
            } else {
                break;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// =============================================================================
// Max Heap (wraps MinHeap with reversed ordering)
// =============================================================================

use std::cmp::Reverse;

pub struct MaxHeap<T: Ord> {
    inner: MinHeap<Reverse<T>>,
}

impl<T: Ord> MaxHeap<T> {
    pub fn new() -> Self {
        Self {
            inner: MinHeap::new(),
        }
    }

    pub fn push(&mut self, value: T) {
        self.inner.push(Reverse(value));
    }

    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop().map(|r| r.0)
    }

    pub fn peek(&self) -> Option<&T> {
        self.inner.peek().map(|r| &r.0)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

// =============================================================================
// Priority Queue (key-value)
// =============================================================================

struct PqEntry<P: Ord, V> {
    priority: P,
    value: V,
}

impl<P: Ord, V> Ord for PqEntry<P, V> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl<P: Ord, V> PartialOrd for PqEntry<P, V> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<P: Ord, V> PartialEq for PqEntry<P, V> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl<P: Ord, V> Eq for PqEntry<P, V> {}

/// Min-priority queue: lowest priority value dequeued first.
pub struct PriorityQueue<P: Ord, V> {
    heap: MinHeap<PqEntry<P, V>>,
}

impl<P: Ord, V> PriorityQueue<P, V> {
    pub fn new() -> Self {
        Self {
            heap: MinHeap::new(),
        }
    }

    pub fn enqueue(&mut self, priority: P, value: V) {
        self.heap.push(PqEntry { priority, value });
    }

    pub fn dequeue(&mut self) -> Option<(P, V)> {
        self.heap.pop().map(|e| (e.priority, e.value))
    }

    pub fn peek(&self) -> Option<(&P, &V)> {
        self.heap.peek().map(|e| (&e.priority, &e.value))
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

// =============================================================================
// Heapsort
// =============================================================================

pub fn heapsort<T: Ord>(data: &mut [T]) {
    let n = data.len();
    if n <= 1 {
        return;
    }

    // Build max-heap (reverse comparisons for ascending sort)
    for i in (0..n / 2).rev() {
        sift_down_max(data, i, n);
    }

    // Extract max repeatedly
    for end in (1..n).rev() {
        data.swap(0, end);
        sift_down_max(data, 0, end);
    }
}

fn sift_down_max<T: Ord>(data: &mut [T], mut idx: usize, n: usize) {
    loop {
        let left = 2 * idx + 1;
        let right = 2 * idx + 2;
        let mut largest = idx;

        if left < n && data[left] > data[largest] {
            largest = left;
        }
        if right < n && data[right] > data[largest] {
            largest = right;
        }

        if largest != idx {
            data.swap(idx, largest);
            idx = largest;
        } else {
            break;
        }
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Min Heap ===");
    let mut heap = MinHeap::from_vec(vec![5, 3, 8, 1, 2, 7]);
    print!("Sorted extract: ");
    while let Some(v) = heap.pop() {
        print!("{v} ");
    }
    println!();

    println!("\n=== Max Heap ===");
    let mut mh = MaxHeap::new();
    for v in [3, 1, 4, 1, 5, 9, 2, 6] {
        mh.push(v);
    }
    print!("Max extract: ");
    while let Some(v) = mh.pop() {
        print!("{v} ");
    }
    println!();

    println!("\n=== Priority Queue ===");
    let mut pq = PriorityQueue::new();
    pq.enqueue(3, "low priority");
    pq.enqueue(1, "high priority");
    pq.enqueue(2, "medium priority");
    while let Some((p, v)) = pq.dequeue() {
        println!("  priority={p}: {v}");
    }

    println!("\n=== Heapsort ===");
    let mut data = vec![38, 27, 43, 3, 9, 82, 10];
    heapsort(&mut data);
    println!("Sorted: {data:?}");
}
