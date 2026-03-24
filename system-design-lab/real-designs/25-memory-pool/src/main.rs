//! # High-Performance Memory Pool — Single Thread
//!
//! Implements and benchmarks three allocator designs:
//! 1. Fixed-size slab pool (free-list, O(1) alloc/free)
//! 2. Size-class pool (multiple slabs for different sizes)
//! 3. Arena/bump allocator (pointer bump, free all at once)
//!
//! Compares against system allocator (Box::new / Vec::with_capacity).
//!
//! Run: cargo run -p memory-pool --release

use std::time::Instant;

// =============================================================================
// 1. Fixed-Size Slab Pool
// =============================================================================

// Pre-allocate N blocks of fixed size. Free list = stack of indices.
//
//   Memory layout:
//   ┌────────┬────────┬────────┬────────┬────────┐
//   │ Block0 │ Block1 │ Block2 │ Block3 │ Block4 │  ← contiguous memory
//   └────────┴────────┴────────┴────────┴────────┘
//
//   Free list (stack):  [4, 3, 2, 1, 0]  ← all free initially
//   alloc() = pop(0)  → return Block0, free list = [4, 3, 2, 1]
//   alloc() = pop(1)  → return Block1, free list = [4, 3, 2]
//   free(0) = push(0) → free list = [4, 3, 2, 0]
//
//   Both alloc and free are O(1): just push/pop from a Vec (stack).

struct SlabPool {
    memory: Vec<u8>,       // raw memory backing store
    block_size: usize,     // bytes per block
    capacity: usize,       // total number of blocks
    free_list: Vec<usize>, // stack of free block indices
    allocated: usize,      // count of currently allocated blocks
}

impl SlabPool {
    fn new(block_size: usize, capacity: usize) -> Self {
        let memory = vec![0u8; block_size * capacity];
        let free_list: Vec<usize> = (0..capacity).rev().collect(); // all blocks free
        Self {
            memory,
            block_size,
            capacity,
            free_list,
            allocated: 0,
        }
    }

    // O(1) allocation: pop index from free list, return pointer to block
    fn alloc(&mut self) -> Option<*mut u8> {
        let index = self.free_list.pop()?;
        self.allocated += 1;
        let offset = index * self.block_size;
        Some(unsafe { self.memory.as_mut_ptr().add(offset) })
    }

    // O(1) free: push index back to free list
    fn free(&mut self, ptr: *mut u8) {
        let offset = unsafe { ptr.offset_from(self.memory.as_ptr()) } as usize;
        let index = offset / self.block_size;
        debug_assert!(index < self.capacity, "invalid pointer");
        self.free_list.push(index);
        self.allocated -= 1;
    }

    fn available(&self) -> usize {
        self.free_list.len()
    }
}

// =============================================================================
// 2. Size-Class Pool
// =============================================================================

// Multiple slab pools for different size classes.
// alloc(n) → round up to nearest size class → alloc from that pool.
//
//   Size classes:
//     Class 0:   16-byte blocks
//     Class 1:   64-byte blocks
//     Class 2:  256-byte blocks
//     Class 3: 1024-byte blocks
//     Class 4: 4096-byte blocks

struct SizeClassPool {
    pools: Vec<(usize, SlabPool)>, // (max_size, pool)
}

impl SizeClassPool {
    fn new(blocks_per_class: usize) -> Self {
        let size_classes = [16, 64, 256, 1024, 4096];
        let pools = size_classes.iter()
            .map(|&size| (size, SlabPool::new(size, blocks_per_class)))
            .collect();
        Self { pools }
    }

    fn alloc(&mut self, size: usize) -> Option<(*mut u8, usize)> {
        // Find the smallest size class that fits
        for (class_size, pool) in &mut self.pools {
            if size <= *class_size {
                let ptr = pool.alloc()?;
                return Some((ptr, *class_size));
            }
        }
        None // too large for any pool
    }

    fn free(&mut self, ptr: *mut u8, class_size: usize) {
        for (size, pool) in &mut self.pools {
            if *size == class_size {
                pool.free(ptr);
                return;
            }
        }
    }
}

// =============================================================================
// 3. Arena (Bump) Allocator
// =============================================================================

// Allocate by bumping a pointer forward. Never free individually.
// Drop the entire arena to reclaim all memory at once.
//
//   ┌─────────────────────────────────────────────┐
//   │████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░│
//   └────────────────▲────────────────────────────┘
//                    │
//                 offset (bump forward)
//
//   alloc(32) → ptr = base + offset; offset += 32;  // O(1), no free list
//   reset()   → offset = 0;                          // "free everything" in O(1)

struct Arena {
    memory: Vec<u8>,
    offset: usize,
    alloc_count: usize,
}

impl Arena {
    fn new(capacity: usize) -> Self {
        Self {
            memory: vec![0u8; capacity],
            offset: 0,
            alloc_count: 0,
        }
    }

    // O(1) allocation: just bump the pointer
    fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        // Align the offset
        let aligned_offset = (self.offset + align - 1) & !(align - 1);
        if aligned_offset + size > self.memory.len() {
            return None; // out of space
        }
        let ptr = unsafe { self.memory.as_mut_ptr().add(aligned_offset) };
        self.offset = aligned_offset + size;
        self.alloc_count += 1;
        Some(ptr)
    }

    // O(1) "free everything" — just reset the pointer
    fn reset(&mut self) {
        self.offset = 0;
        self.alloc_count = 0;
    }

    fn used(&self) -> usize {
        self.offset
    }

    fn remaining(&self) -> usize {
        self.memory.len() - self.offset
    }
}

// =============================================================================
// Demo + Benchmarks
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║   High-Performance Memory Pool (Single Thread)   ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // ── 1. Slab Pool Demo ──
    println!("━━━ 1. Fixed-Size Slab Pool ━━━\n");
    {
        let mut pool = SlabPool::new(64, 1000); // 1000 blocks of 64 bytes
        println!("    Pool: 1000 × 64-byte blocks ({} KB total)", 1000 * 64 / 1024);
        println!("    Free blocks: {}\n", pool.available());

        // Allocate some blocks
        let mut ptrs = Vec::new();
        for _ in 0..5 {
            let ptr = pool.alloc().unwrap();
            ptrs.push(ptr);
        }
        println!("    Allocated 5 blocks");
        println!("    Free blocks: {}, In use: {}\n", pool.available(), pool.allocated);

        // Free them back
        for ptr in ptrs {
            pool.free(ptr);
        }
        println!("    Freed all 5 blocks");
        println!("    Free blocks: {}, In use: {}\n", pool.available(), pool.allocated);
    }

    // ── 2. Size-Class Pool Demo ──
    println!("━━━ 2. Size-Class Pool ━━━\n");
    {
        let mut pool = SizeClassPool::new(1000); // 1000 blocks per size class

        println!("    Size classes: 16, 64, 256, 1024, 4096 bytes\n");

        let (ptr1, class1) = pool.alloc(10).unwrap();   // 10 bytes → 16-byte class
        let (ptr2, class2) = pool.alloc(50).unwrap();   // 50 bytes → 64-byte class
        let (ptr3, class3) = pool.alloc(200).unwrap();  // 200 bytes → 256-byte class
        let (ptr4, class4) = pool.alloc(500).unwrap();  // 500 bytes → 1024-byte class

        println!("    alloc(10 bytes)  → {}-byte block (waste: {} bytes)", class1, class1 - 10);
        println!("    alloc(50 bytes)  → {}-byte block (waste: {} bytes)", class2, class2 - 50);
        println!("    alloc(200 bytes) → {}-byte block (waste: {} bytes)", class3, class3 - 200);
        println!("    alloc(500 bytes) → {}-byte block (waste: {} bytes)\n", class4, class4 - 500);

        println!("    Internal fragmentation: up to ~50% per allocation.");
        println!("    But: zero external fragmentation, O(1) alloc/free.\n");

        pool.free(ptr1, class1);
        pool.free(ptr2, class2);
        pool.free(ptr3, class3);
        pool.free(ptr4, class4);
    }

    // ── 3. Arena Demo ──
    println!("━━━ 3. Arena (Bump) Allocator ━━━\n");
    {
        let mut arena = Arena::new(4096); // 4KB arena

        println!("    Arena: 4096 bytes, used: {}, remaining: {}\n", arena.used(), arena.remaining());

        // Simulate a request: allocate headers, body, response
        let _headers = arena.alloc(128, 8).unwrap();
        println!("    alloc(128) for headers  → used: {}", arena.used());

        let _body = arena.alloc(512, 8).unwrap();
        println!("    alloc(512) for body     → used: {}", arena.used());

        let _response = arena.alloc(256, 8).unwrap();
        println!("    alloc(256) for response → used: {}", arena.used());

        println!("    Allocations: {}, remaining: {} bytes\n", arena.alloc_count, arena.remaining());

        // Request done → free everything at once
        arena.reset();
        println!("    arena.reset() → used: {}, remaining: {}", arena.used(), arena.remaining());
        println!("    All memory reclaimed in O(1). No per-object free needed.\n");
    }

    // ── 4. Benchmark: Pool vs System Allocator ──
    println!("━━━ 4. Benchmark — Pool vs System Allocator ━━━\n");

    let iterations = 1_000_000;

    // Benchmark: Slab pool alloc + free
    {
        let mut pool = SlabPool::new(64, iterations);
        let start = Instant::now();
        for _ in 0..iterations {
            let ptr = pool.alloc().unwrap();
            pool.free(ptr);
        }
        let pool_time = start.elapsed();
        println!("    Slab pool (alloc+free × {}): {:?} ({:.1}ns/op)",
            iterations, pool_time, pool_time.as_nanos() as f64 / iterations as f64);
    }

    // Benchmark: System allocator (Box)
    {
        let start = Instant::now();
        for _ in 0..iterations {
            let b = Box::new([0u8; 64]);
            drop(b);
        }
        let sys_time = start.elapsed();
        println!("    System alloc (Box × {}):     {:?} ({:.1}ns/op)",
            iterations, sys_time, sys_time.as_nanos() as f64 / iterations as f64);
    }

    // Benchmark: Arena bump alloc
    {
        let mut arena = Arena::new(64 * iterations);
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = arena.alloc(64, 8).unwrap();
        }
        let arena_time = start.elapsed();
        println!("    Arena bump (alloc × {}):     {:?} ({:.1}ns/op)",
            iterations, arena_time, arena_time.as_nanos() as f64 / iterations as f64);
    }

    // Benchmark: Arena reset (free all at once)
    {
        let mut arena = Arena::new(64 * 1000);
        // Fill it, then reset 1M times
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = arena.alloc(64, 8);
            if arena.remaining() < 64 {
                arena.reset(); // free everything in O(1)
            }
        }
        let arena_reset_time = start.elapsed();
        println!("    Arena fill+reset (× {}):     {:?} ({:.1}ns/op)\n",
            iterations, arena_reset_time, arena_reset_time.as_nanos() as f64 / iterations as f64);
    }

    // ── Summary ──
    println!("    ┌──────────────────┬─────────┬─────────┬───────────────────────┐");
    println!("    │ Allocator        │ alloc   │ free    │ Best for              │");
    println!("    ├──────────────────┼─────────┼─────────┼───────────────────────┤");
    println!("    │ Slab pool        │ O(1)    │ O(1)    │ Fixed-size objects    │");
    println!("    │ Size-class pool  │ O(1)    │ O(1)    │ Mixed sizes           │");
    println!("    │ Arena (bump)     │ O(1)    │ N/A*    │ Request-scoped alloc  │");
    println!("    │ System (malloc)  │ O(log n)│ O(log n)│ General purpose       │");
    println!("    └──────────────────┴─────────┴─────────┴───────────────────────┘");
    println!("    * Arena: no individual free. reset() frees everything at once.\n");

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
