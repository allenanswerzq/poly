#![allow(dead_code, unused_variables, unused_imports)]
//! # Lock-Free Data Structures
//!
//! - Treiber stack (lock-free stack using CAS)
//! - Lock-free atomic counter
//! - SPSC ring buffer (single-producer, single-consumer)

use std::sync::atomic::{AtomicPtr, AtomicUsize, AtomicBool, Ordering};
use std::sync::Arc;
use std::ptr;
use std::thread;

// =============================================================================
// Treiber Stack (Lock-Free Stack)
// =============================================================================
// Classic lock-free stack using compare-and-swap on the head pointer.
//
// Push: loop { new_node.next = head; CAS(head, new_node.next, new_node) }
// Pop:  loop { old = head; CAS(head, old, old.next) }
//
// Note: In production, use crossbeam-epoch for safe memory reclamation.
// This demo leaks popped nodes for simplicity — real implementations need
// epoch-based reclamation or hazard pointers.

struct TreiberNode<T> {
    value: T,
    next: *mut TreiberNode<T>,
}

pub struct TreiberStack<T> {
    head: AtomicPtr<TreiberNode<T>>,
}

unsafe impl<T: Send> Send for TreiberStack<T> {}
unsafe impl<T: Send> Sync for TreiberStack<T> {}

impl<T> TreiberStack<T> {
    pub fn new() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }

    pub fn push(&self, value: T) {
        let new_node = Box::into_raw(Box::new(TreiberNode {
            value,
            next: ptr::null_mut(),
        }));

        loop {
            let old_head = self.head.load(Ordering::Acquire);
            unsafe {
                (*new_node).next = old_head;
            }
            // CAS: if head is still old_head, swap to new_node
            if self
                .head
                .compare_exchange_weak(old_head, new_node, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            // Otherwise retry — another thread modified head
        }
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let old_head = self.head.load(Ordering::Acquire);
            if old_head.is_null() {
                return None;
            }
            let next = unsafe { (*old_head).next };
            if self
                .head
                .compare_exchange_weak(old_head, next, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                let value = unsafe { Box::from_raw(old_head).value };
                return Some(value);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire).is_null()
    }
}

impl<T> Drop for TreiberStack<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

// =============================================================================
// Lock-Free Atomic Counter
// =============================================================================
// Demonstrates fetch_add, compare_exchange, etc.

pub struct AtomicCounter {
    value: AtomicUsize,
}

impl AtomicCounter {
    pub fn new(initial: usize) -> Self {
        Self {
            value: AtomicUsize::new(initial),
        }
    }

    pub fn increment(&self) -> usize {
        self.value.fetch_add(1, Ordering::SeqCst)
    }

    pub fn decrement(&self) -> usize {
        self.value.fetch_sub(1, Ordering::SeqCst)
    }

    pub fn get(&self) -> usize {
        self.value.load(Ordering::SeqCst)
    }

    /// CAS-based: increment only if current value < max.
    pub fn increment_if_below(&self, max: usize) -> bool {
        loop {
            let current = self.value.load(Ordering::Acquire);
            if current >= max {
                return false;
            }
            if self
                .value
                .compare_exchange_weak(current, current + 1, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }
}

// =============================================================================
// SPSC Ring Buffer (Single-Producer, Single-Consumer)
// =============================================================================
// Lock-free bounded queue for exactly one producer and one consumer thread.
// Uses relaxed memory ordering between the two cache lines.

pub struct SpscQueue<T> {
    buffer: Vec<std::cell::UnsafeCell<Option<T>>>,
    capacity: usize,
    head: AtomicUsize, // consumer reads here
    tail: AtomicUsize, // producer writes here
}

unsafe impl<T: Send> Send for SpscQueue<T> {}
unsafe impl<T: Send> Sync for SpscQueue<T> {}

impl<T> SpscQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(std::cell::UnsafeCell::new(None));
        }
        Self {
            buffer,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Producer: try to enqueue.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = (tail + 1) % self.capacity;
        let head = self.head.load(Ordering::Acquire);

        if next_tail == head {
            return Err(value); // full
        }

        unsafe {
            *self.buffer[tail].get() = Some(value);
        }
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }

    /// Consumer: try to dequeue.
    pub fn try_pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None; // empty
        }

        let value = unsafe { (*self.buffer[head].get()).take() };
        self.head
            .store((head + 1) % self.capacity, Ordering::Release);
        value
    }

    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

// =============================================================================
// AtomicFlag (Spin Lock — for educational purposes)
// =============================================================================
// A simple spin lock built from AtomicBool. NOT recommended for production
// (waste CPU cycles), but good to understand the concept.

pub struct SpinLock {
    locked: AtomicBool,
}

impl SpinLock {
    pub fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // spin — hint to the CPU this is a spin loop
            std::hint::spin_loop();
        }
    }

    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Treiber Stack (Lock-Free) ===");
    let stack = Arc::new(TreiberStack::new());
    let mut handles = Vec::new();

    // 4 threads push 25 items each
    for t in 0..4 {
        let stack = Arc::clone(&stack);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                stack.push(t * 100 + i);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let mut count = 0;
    while stack.pop().is_some() {
        count += 1;
    }
    println!("Pushed by 4 threads, popped {count} items (expected 100)");

    println!("\n=== Atomic Counter ===");
    let counter = Arc::new(AtomicCounter::new(0));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                counter.increment();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!(
        "8 threads x 1000 increments = {} (expected 8000)",
        counter.get()
    );

    println!("\n=== SPSC Ring Buffer ===");
    let queue = Arc::new(SpscQueue::new(64));
    let q1 = Arc::clone(&queue);
    let q2 = Arc::clone(&queue);

    let producer = thread::spawn(move || {
        for i in 0..50 {
            while q1.try_push(i).is_err() {
                std::hint::spin_loop();
            }
        }
    });

    let consumer = thread::spawn(move || {
        let mut sum = 0u64;
        for _ in 0..50 {
            loop {
                if let Some(v) = q2.try_pop() {
                    sum += v;
                    break;
                }
                std::hint::spin_loop();
            }
        }
        sum
    });

    producer.join().unwrap();
    let sum = consumer.join().unwrap();
    println!("Sum of 0..50 through SPSC queue: {sum} (expected {})", (0..50u64).sum::<u64>());

    println!("\n=== Spin Lock ===");
    let lock = Arc::new(SpinLock::new());
    let shared = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..4 {
        let lock = Arc::clone(&lock);
        let shared = Arc::clone(&shared);
        handles.push(thread::spawn(move || {
            for _ in 0..1000 {
                lock.lock();
                shared.fetch_add(1, Ordering::Relaxed);
                lock.unlock();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    println!(
        "Spin lock protected counter: {} (expected 4000)",
        shared.load(Ordering::SeqCst)
    );
}
