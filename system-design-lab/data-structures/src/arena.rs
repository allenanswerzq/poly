#![allow(dead_code, unused_variables, unused_imports)]
//! # Arena Allocator & Object Pool
//!
//! - Arena: bump allocator for batch allocation, one-shot deallocation
//! - Object Pool: reuse pre-allocated objects to avoid allocation overhead
//! - TypedArena: type-safe arena for a single type

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// =============================================================================
// Simple Arena Allocator (Bump Allocator)
// =============================================================================
// Fastest possible allocation strategy: just bump a pointer.
// No individual deallocation — everything freed at once when the arena is dropped.
// Perfect for request-scoped allocations (web servers, compilers, game frames).

pub struct Arena {
    chunks: Vec<Vec<u8>>,
    current: usize,     // index into current chunk
    chunk_size: usize,
}

impl Arena {
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunks: vec![vec![0u8; chunk_size]],
            current: 0,
            chunk_size,
        }
    }

    /// Allocate `size` bytes, returning a mutable slice.
    pub fn alloc(&mut self, size: usize) -> &mut [u8] {
        if self.current + size > self.chunks.last().unwrap().len() {
            // Need a new chunk
            let new_size = self.chunk_size.max(size);
            self.chunks.push(vec![0u8; new_size]);
            self.current = 0;
        }
        let chunk = self.chunks.last_mut().unwrap();
        let start = self.current;
        self.current += size;
        &mut chunk[start..start + size]
    }

    /// Reset the arena without freeing memory (reuse buffers).
    pub fn reset(&mut self) {
        // Keep only the first chunk, reset offset
        self.chunks.truncate(1);
        self.current = 0;
    }

    pub fn total_allocated(&self) -> usize {
        if self.chunks.is_empty() {
            return 0;
        }
        (self.chunks.len() - 1) * self.chunk_size + self.current
    }
}

// =============================================================================
// Typed Arena
// =============================================================================
// Allocates objects of a single type. Returns references that are valid
// for the lifetime of the arena.

pub struct TypedArena<T> {
    chunks: Vec<Vec<T>>,
    chunk_size: usize,
}

impl<T> TypedArena<T> {
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunks: Vec::new(),
            chunk_size,
        }
    }

    pub fn alloc(&mut self, value: T) -> &mut T {
        // Check if current chunk has space
        let needs_new = self
            .chunks
            .last()
            .map_or(true, |chunk| chunk.len() >= self.chunk_size);

        if needs_new {
            self.chunks.push(Vec::with_capacity(self.chunk_size));
        }

        let chunk = self.chunks.last_mut().unwrap();
        chunk.push(value);
        chunk.last_mut().unwrap()
    }

    pub fn total_count(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum()
    }
}

// =============================================================================
// Object Pool
// =============================================================================
// Pre-allocate objects and reuse them. Avoids allocation/deallocation overhead
// for frequently created/destroyed objects (connections, buffers, game entities).

pub struct ObjectPool<T> {
    objects: Vec<T>,
    available: Vec<usize>, // indices of available objects
}

impl<T: Default + Clone> ObjectPool<T> {
    pub fn new(size: usize) -> Self {
        let objects: Vec<T> = (0..size).map(|_| T::default()).collect();
        let available: Vec<usize> = (0..size).collect();
        Self { objects, available }
    }

    /// Acquire an object from the pool.
    pub fn acquire(&mut self) -> Option<(usize, &mut T)> {
        let idx = self.available.pop()?;
        Some((idx, &mut self.objects[idx]))
    }

    /// Release an object back to the pool.
    pub fn release(&mut self, idx: usize) {
        self.objects[idx] = T::default(); // reset to default state
        self.available.push(idx);
    }

    pub fn available_count(&self) -> usize {
        self.available.len()
    }

    pub fn total_capacity(&self) -> usize {
        self.objects.len()
    }
}

// =============================================================================
// Thread-Safe Object Pool
// =============================================================================

pub struct ConcurrentPool<T> {
    objects: Vec<Mutex<Option<T>>>,
    free_list: Mutex<Vec<usize>>,
}

impl<T: Default> ConcurrentPool<T> {
    pub fn new(size: usize) -> Self {
        let objects: Vec<_> = (0..size)
            .map(|_| Mutex::new(Some(T::default())))
            .collect();
        let free_list = Mutex::new((0..size).collect());
        Self { objects, free_list }
    }

    /// Acquire an object. Returns (index, object).
    pub fn acquire(&self) -> Option<(usize, T)> {
        let idx = self.free_list.lock().ok()?.pop()?;
        let obj = self.objects[idx].lock().ok()?.take()?;
        Some((idx, obj))
    }

    /// Return an object to the pool.
    pub fn release(&self, idx: usize, obj: T) {
        if let Ok(mut slot) = self.objects[idx].lock() {
            *slot = Some(obj);
        }
        if let Ok(mut free) = self.free_list.lock() {
            free.push(idx);
        }
    }

    pub fn available(&self) -> usize {
        self.free_list.lock().map_or(0, |f| f.len())
    }
}

// =============================================================================
// Slab Allocator
// =============================================================================
// Like an arena + free list. Allocate by index, O(1) insert and remove.
// Used in tokio, mio, and other async runtimes.

pub struct Slab<T> {
    entries: Vec<Option<T>>,
    free: Vec<usize>,
    len: usize,
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            free: Vec::new(),
            len: 0,
        }
    }

    /// Insert a value and return its key (index).
    pub fn insert(&mut self, value: T) -> usize {
        self.len += 1;
        if let Some(idx) = self.free.pop() {
            self.entries[idx] = Some(value);
            idx
        } else {
            let idx = self.entries.len();
            self.entries.push(Some(value));
            idx
        }
    }

    /// Remove the value at key and return it.
    pub fn remove(&mut self, key: usize) -> Option<T> {
        let value = self.entries.get_mut(key)?.take()?;
        self.free.push(key);
        self.len -= 1;
        Some(value)
    }

    pub fn get(&self, key: usize) -> Option<&T> {
        self.entries.get(key)?.as_ref()
    }

    pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
        self.entries.get_mut(key)?.as_mut()
    }

    pub fn contains(&self, key: usize) -> bool {
        self.entries.get(key).map_or(false, |e| e.is_some())
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Arena Allocator ===");
    let mut arena = Arena::new(1024);
    let buf1 = arena.alloc(100);
    buf1[0] = 42;
    let buf2 = arena.alloc(200);
    buf2[0] = 99;
    println!("Allocated {} bytes total", arena.total_allocated());
    arena.reset();
    println!("After reset: {} bytes used", arena.total_allocated());

    println!("\n=== Typed Arena ===");
    let mut typed = TypedArena::new(64);
    for i in 0..100 {
        typed.alloc(format!("item-{i}"));
    }
    println!("Allocated {} strings in typed arena", typed.total_count());

    println!("\n=== Object Pool ===");
    let mut pool: ObjectPool<Vec<u8>> = ObjectPool::new(5);
    println!("Available: {}/{}", pool.available_count(), pool.total_capacity());
    let (idx, buf) = pool.acquire().unwrap();
    buf.extend_from_slice(b"hello");
    println!("Acquired slot {idx}, buf = {:?}", buf);
    println!("Available: {}", pool.available_count());
    pool.release(idx);
    println!("After release, available: {}", pool.available_count());

    println!("\n=== Slab Allocator ===");
    let mut slab = Slab::new();
    let k1 = slab.insert("hello");
    let k2 = slab.insert("world");
    let k3 = slab.insert("foo");
    println!("Inserted at keys: {k1}, {k2}, {k3}");
    println!("slab[{k1}] = {:?}", slab.get(k1));
    slab.remove(k1);
    let k4 = slab.insert("reuse"); // should reuse slot k1
    println!("After remove+insert: key={k4}, value={:?}", slab.get(k4));
    println!("Slab len: {}", slab.len());
}
