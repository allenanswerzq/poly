#![allow(dead_code, unused_variables, unused_imports)]
//! # Stack, Queue, Deque
//!
//! - Stack: LIFO, Vec-backed
//! - Queue: FIFO, ring buffer (circular array)
//! - Deque: double-ended, ring buffer

// =============================================================================
// Stack (Vec-backed)
// =============================================================================

pub struct Stack<T> {
    data: Vec<T>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.data.pop()
    }

    pub fn peek(&self) -> Option<&T> {
        self.data.last()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// =============================================================================
// Min Stack — supports O(1) min()
// Classic interview question
// =============================================================================

pub struct MinStack<T: Ord + Clone> {
    data: Vec<T>,
    mins: Vec<T>, // parallel stack tracking minimums
}

impl<T: Ord + Clone> MinStack<T> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            mins: Vec::new(),
        }
    }

    pub fn push(&mut self, value: T) {
        if self.mins.is_empty() || value <= *self.mins.last().unwrap() {
            self.mins.push(value.clone());
        }
        self.data.push(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        let val = self.data.pop()?;
        if Some(&val) == self.mins.last() {
            self.mins.pop();
        }
        Some(val)
    }

    pub fn min(&self) -> Option<&T> {
        self.mins.last()
    }

    pub fn peek(&self) -> Option<&T> {
        self.data.last()
    }
}

// =============================================================================
// Queue (Ring Buffer)
// =============================================================================

/// Fixed-capacity ring buffer queue.
pub struct RingQueue<T> {
    buf: Vec<Option<T>>,
    head: usize, // read pointer
    tail: usize, // write pointer
    len: usize,
    cap: usize,
}

impl<T> RingQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buf = Vec::with_capacity(capacity);
        buf.resize_with(capacity, || None);
        Self {
            buf,
            head: 0,
            tail: 0,
            len: 0,
            cap: capacity,
        }
    }

    pub fn enqueue(&mut self, value: T) -> Result<(), &'static str> {
        if self.len == self.cap {
            return Err("queue is full");
        }
        self.buf[self.tail] = Some(value);
        self.tail = (self.tail + 1) % self.cap;
        self.len += 1;
        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let value = self.buf[self.head].take();
        self.head = (self.head + 1) % self.cap;
        self.len -= 1;
        value
    }

    pub fn peek(&self) -> Option<&T> {
        self.buf[self.head].as_ref()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == self.cap
    }
}

// =============================================================================
// Growable Queue (Deque-style, VecDeque from scratch)
// =============================================================================

/// A growable double-ended queue using a ring buffer.
pub struct Deque<T> {
    buf: Vec<Option<T>>,
    head: usize,
    len: usize,
}

impl<T> Deque<T> {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            head: 0,
            len: 0,
        }
    }

    fn cap(&self) -> usize {
        self.buf.len()
    }

    fn grow(&mut self) {
        let old_cap = self.cap();
        let new_cap = if old_cap == 0 { 4 } else { old_cap * 2 };
        let mut new_buf = Vec::with_capacity(new_cap);
        new_buf.resize_with(new_cap, || None);

        // Copy elements in order
        for i in 0..self.len {
            let old_idx = (self.head + i) % old_cap;
            new_buf[i] = self.buf[old_idx].take();
        }
        self.buf = new_buf;
        self.head = 0;
    }

    fn wrap_idx(&self, idx: usize) -> usize {
        if self.cap() == 0 {
            0
        } else {
            idx % self.cap()
        }
    }

    pub fn push_back(&mut self, value: T) {
        if self.len == self.cap() {
            self.grow();
        }
        let idx = self.wrap_idx(self.head + self.len);
        self.buf[idx] = Some(value);
        self.len += 1;
    }

    pub fn push_front(&mut self, value: T) {
        if self.len == self.cap() {
            self.grow();
        }
        self.head = if self.head == 0 {
            self.cap() - 1
        } else {
            self.head - 1
        };
        self.buf[self.head] = Some(value);
        self.len += 1;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let value = self.buf[self.head].take();
        self.head = self.wrap_idx(self.head + 1);
        self.len -= 1;
        value
    }

    pub fn pop_back(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let idx = self.wrap_idx(self.head + self.len - 1);
        let value = self.buf[idx].take();
        self.len -= 1;
        value
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }
        self.buf[self.wrap_idx(self.head + index)].as_ref()
    }
}

// =============================================================================
// Monotonic Stack — useful for "next greater element" type problems
// =============================================================================

/// Returns for each element the index of the next greater element, or None.
pub fn next_greater_element(nums: &[i32]) -> Vec<Option<usize>> {
    let n = nums.len();
    let mut result = vec![None; n];
    let mut stack: Vec<usize> = Vec::new(); // stack of indices

    for i in 0..n {
        while let Some(&top) = stack.last() {
            if nums[top] < nums[i] {
                result[top] = Some(i);
                stack.pop();
            } else {
                break;
            }
        }
        stack.push(i);
    }
    result
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Stack ===");
    let mut stack = Stack::new();
    stack.push(1);
    stack.push(2);
    stack.push(3);
    println!("Pop: {:?}", stack.pop());
    println!("Peek: {:?}", stack.peek());

    println!("\n=== Min Stack ===");
    let mut ms = MinStack::new();
    ms.push(3);
    ms.push(1);
    ms.push(2);
    println!("Min: {:?}", ms.min());
    ms.pop();
    ms.pop();
    println!("Min after pops: {:?}", ms.min());

    println!("\n=== Ring Queue ===");
    let mut q = RingQueue::new(3);
    q.enqueue(1).unwrap();
    q.enqueue(2).unwrap();
    q.enqueue(3).unwrap();
    println!("Full: {}", q.is_full());
    println!("Dequeue: {:?}", q.dequeue());
    q.enqueue(4).unwrap();
    while let Some(v) = q.dequeue() {
        print!("{v} ");
    }
    println!();

    println!("\n=== Deque ===");
    let mut dq = Deque::new();
    dq.push_back(1);
    dq.push_back(2);
    dq.push_front(0);
    dq.push_front(-1);
    while let Some(v) = dq.pop_front() {
        print!("{v} ");
    }
    println!();

    println!("\n=== Monotonic Stack (next greater element) ===");
    let nums = vec![2, 1, 2, 4, 3];
    let nge = next_greater_element(&nums);
    for (i, v) in nge.iter().enumerate() {
        println!("  nums[{i}]={} -> next greater at {:?}", nums[i], v);
    }
}
