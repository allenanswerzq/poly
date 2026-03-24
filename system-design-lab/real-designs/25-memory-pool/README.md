# Design a Single-Thread High-Performance Memory Pool

## Problem Statement

Design a memory allocator that is faster than `malloc`/`free` for a specific workload. Used in game engines, trading systems, network servers, and databases where allocation latency matters.

## Why Not Just Use malloc?

```
malloc (general purpose):
  - Must handle ANY size (1 byte to 1GB)
  - Must be thread-safe (locks or atomic ops)
  - Must handle fragmentation
  - Must search free lists
  - Cost: ~50-100ns per allocation

Custom pool (fixed size, single thread):
  - Fixed block size → no search
  - Single thread → no locks
  - Pre-allocated → no syscalls
  - Free list = stack → O(1) push/pop
  - Cost: ~5-10ns per allocation (10x faster)
```

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    Memory Pool                            │
│                                                           │
│  Pre-allocate a large chunk of memory at startup.        │
│  Divide into fixed-size blocks. Maintain a free list.     │
│                                                           │
│  ┌───────┬───────┬───────┬───────┬───────┬───────┐      │
│  │ Block │ Block │ Block │ Block │ Block │ Block │      │
│  │  64B  │  64B  │  64B  │  64B  │  64B  │  64B  │      │
│  └───┬───┴───────┴───┬───┴───────┴───┬───┴───────┘      │
│      │               │               │                    │
│      ▼               ▼               ▼                    │
│  [in use]         [FREE]          [FREE]                 │
│                      │               │                    │
│  Free list:     head ──► block 2 ──► block 4 ──► NULL    │
│  (stack-like: alloc = pop, free = push, both O(1))       │
│                                                           │
└──────────────────────────────────────────────────────────┘
```

## Three Pool Designs

### 1. Fixed-Size Pool (Slab Allocator)
```
All blocks are the same size. Fastest possible.

Use case: network server allocating 1000-byte packet buffers
  - Pre-allocate 10,000 × 1KB blocks
  - alloc() = pop from free list: O(1), ~5ns
  - free()  = push to free list: O(1), ~5ns

Fragmentation: ZERO (all blocks same size)
```

### 2. Size-Class Pool (like jemalloc/tcmalloc)
```
Multiple pools for different size classes:

  Size class 0:  8-byte blocks   (tiny objects)
  Size class 1:  64-byte blocks  (small structs)
  Size class 2:  256-byte blocks (medium)
  Size class 3:  1024-byte blocks (large)
  Size class 4:  4096-byte blocks (page-size)

alloc(100 bytes) → round up to 128 → use size class 2
alloc(20 bytes)  → round up to 32  → use size class 1

Internal fragmentation: up to ~50% waste (20 bytes in 32-byte block)
But no external fragmentation and O(1) alloc/free per class.
```

### 3. Arena Allocator (Bump Allocator)
```
Allocate by bumping a pointer. Never free individually.
Free everything at once when the arena is dropped.

  ┌───────────────────────────────────────────┐
  │████████████████████░░░░░░░░░░░░░░░░░░░░░░│
  └───────────────────▲───────────────────────┘
                      │
                   pointer (bump forward on each alloc)

alloc(n) = ptr; ptr += n;  → O(1), literally 1 addition
free() = NOT SUPPORTED (free everything at once)

Use case: request handling (allocate during request, drop all when done)
  - HTTP request: allocate headers, body, response buffer
  - Request completes: drop entire arena (instant, no per-object free)
```

## When to Use Which

| Allocator | alloc | free | Fragmentation | Best for |
|-----------|-------|------|---------------|----------|
| Fixed-size pool | O(1) | O(1) | Zero | Same-size objects (packets, nodes) |
| Size-class pool | O(1) | O(1) | Internal | Mixed sizes, general purpose |
| Arena (bump) | O(1) | N/A | Zero | Batch alloc, free all at once |
| malloc (system) | O(log n) | O(log n) | Both | Everything else |

## Interview Talking Points

> "For the order matching engine, I'd use a fixed-size slab allocator for order objects since they're all the same size. Pre-allocate a pool of 1M order slots. Alloc is a free-list pop (5ns) instead of malloc (50ns). This eliminates allocation jitter in the hot path."

> "For the HTTP server, I'd use an arena allocator per request. Allocate all request-scoped data (headers, body buffer, response) from the arena. When the request completes, drop the entire arena in one operation — no individual frees, no fragmentation."
