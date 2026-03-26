# Performance & Memory Optimization — What Principal Engineers Must Know

## Why This Matters

When the interviewer asks "how would you make this faster?" or "this system handles 1K RPS, how do we get to 100K?", they're testing your performance intuition. The answer is almost always one of the patterns below.

## The Performance Mental Model

```
Slow request arrives. Where is the time spent?

  Network (1-100ms)       ← can you reduce round trips?
    │
    ▼
  Serialization (1-10ms)  ← can you use a faster format?
    │
    ▼
  Application logic       ← can you cache the result?
    │
    ▼
  Database query (1-100ms) ← can you add an index? read replica?
    │
    ▼
  Disk I/O (0.1-10ms)    ← is it SSD? can you keep it in memory?
    │
    ▼
  Response serialization  ← can you stream instead of buffering?

Rule: measure first, optimize the bottleneck. Don't guess.
```

## 1. Memory Optimization

### Know Your Numbers

```
CPU register:       0.3 ns       ← fastest possible
L1 cache:           1 ns         (32-48 KB per core)
L2 cache:           4 ns         (256 KB - 1.25 MB per core)
L3 cache:           12 ns        (30-384 MB shared)
Main memory (RAM):  100 ns       (64 GB - 2 TB)
SSD random read:    100,000 ns   (100 µs)
HDD random read:    10,000,000 ns (10 ms)
Network (same DC):  500,000 ns   (0.5 ms)
Network (cross DC): 50,000,000 ns (50 ms)

Key insight: RAM is 100x slower than L1 cache.
             SSD is 1000x slower than RAM.
             Everything is about keeping data CLOSE to the CPU.
```

### Cache-Friendly Data Structures

```
GOOD: Array / Vec (contiguous memory)
  ┌───┬───┬───┬───┬───┬───┬───┬───┐
  │ 1 │ 2 │ 3 │ 4 │ 5 │ 6 │ 7 │ 8 │  ← all in one cache line (64 bytes)
  └───┴───┴───┴───┴───┴───┴───┴───┘
  Access pattern: sequential → CPU prefetcher loads next cache line ahead
  Every access: cache HIT (1-4 ns)

BAD: Linked list / HashMap with heap-allocated values
  ┌───┐     ┌───┐     ┌───┐
  │ 1 │────►│ 2 │────►│ 3 │  ← each node at random memory location
  └───┘     └───┘     └───┘
  Access pattern: pointer chasing → every node = cache MISS (100 ns)
  100x slower for sequential access!

Interview answer: "I'd use a Vec instead of a LinkedList because
sequential memory access is 100x faster due to CPU cache lines.
Each cache miss costs 100ns, and linked lists miss on every node."
```

### Struct Layout — Padding and Alignment

```
BAD (wastes memory due to padding):
  struct Bad {
      a: u8,     // 1 byte + 7 bytes padding (align to next u64)
      b: u64,    // 8 bytes
      c: u8,     // 1 byte + 7 bytes padding
  }
  // sizeof = 24 bytes (for 10 bytes of actual data!)

GOOD (reorder fields by size, largest first):
  struct Good {
      b: u64,    // 8 bytes
      a: u8,     // 1 byte
      c: u8,     // 1 byte + 6 bytes padding (at the end only)
  }
  // sizeof = 16 bytes (saved 33%)

At scale: 1 billion structs × 8 bytes saved = 8 GB less RAM.

In Rust: #[repr(C)] for predictable layout, or let the compiler optimize.
```

### Arena Allocation

```
Standard allocation:
  alloc(A), alloc(B), alloc(C), free(B), alloc(D), free(A), free(C), free(D)
  → fragmentation, allocator overhead, random memory locations

Arena allocation:
  arena = allocate(1MB block)
  A = arena.alloc(100)    ← just bump a pointer (2ns)
  B = arena.alloc(200)    ← bump pointer again (2ns)
  C = arena.alloc(50)     ← bump pointer (2ns)
  arena.reset()           ← free everything at once (1 instruction!)

  No individual frees. No fragmentation. Data is contiguous (cache-friendly).

  Use for: request-scoped data (HTTP request handler),
           compiler IR (per-function arena), game frames.
```

### Memory Pool / Slab Allocator

```
For fixed-size objects (network packets, connection objects):

  Pool of 1000 pre-allocated 1KB blocks:
  ┌──────┬──────┬──────┬──────┬──────┐
  │ free │ used │ free │ free │ used │ ...
  └──────┴──────┴──────┴──────┴──────┘

  alloc() = pop from free list:  O(1), ~5ns
  free()  = push to free list:   O(1), ~5ns

  vs malloc: O(log n), ~50ns, potential fragmentation

  Used by: Nginx (connection pool), Linux kernel (slab allocator),
           game engines (entity pools), network stacks (packet buffers).
```

## 2. Concurrency Optimization

### Lock-Free Data Structures

```
With locks:
  Thread A: acquire lock → read counter → increment → write → release lock
  Thread B: BLOCKED waiting for lock → acquire → read → increment → write → release
  Threads are serialized. 1 core does useful work, others wait.

Lock-free (atomic):
  Thread A: atomic_add(&counter, 1)   ← hardware instruction, no lock
  Thread B: atomic_add(&counter, 1)   ← simultaneous, both succeed
  Both threads make progress. No blocking.

  Rust: AtomicU64::fetch_add(1, Ordering::Relaxed)

When to use:
  Counters, metrics:        AtomicU64
  Concurrent HashMap:       DashMap (sharded locks) or crossbeam
  Queue (single producer):  crossbeam-channel
  Complex shared state:     RwLock (reader-writer lock, readers don't block each other)
```

### Sharding to Reduce Contention

```
Single shared counter:
  Thread 0 ──┐
  Thread 1 ──┤─► counter (all threads fight for the same atomic)
  Thread 2 ──┤   → cache line bouncing → 100x slower
  Thread 3 ──┘

Sharded counter (per-thread):
  Thread 0 ──► counter_0
  Thread 1 ──► counter_1   (no contention, each thread has its own)
  Thread 2 ──► counter_2
  Thread 3 ──► counter_3

  Total = counter_0 + counter_1 + counter_2 + counter_3
  Read is slightly slower (sum 4 counters), but writes are 100x faster.

  Same pattern for: connection pools (per-thread pool),
  memory allocators (per-thread arena), hash maps (DashMap = sharded HashMap).
```

### False Sharing — The Hidden Performance Killer

```
Two threads on different cores writing to different variables
that happen to be on the SAME cache line (64 bytes):

  Cache line: [counter_a | counter_b | unused padding...]
  Core 0 writes counter_a → invalidates line on Core 1
  Core 1 writes counter_b → invalidates line on Core 0
  → EVERY write forces the other core to reload the line (100ns penalty)

Fix: pad variables to separate cache lines
  #[repr(align(64))]
  struct PaddedCounter {
      value: AtomicU64,    // 8 bytes + 56 bytes padding = 64 bytes total
  }

  Now counter_a and counter_b are on different cache lines → no interference.
```

## 3. I/O Optimization

### Batching

```
BAD: send 1000 individual database queries
  for item in items:
      db.query("SELECT * FROM products WHERE id = ?", item.id)
  → 1000 round trips × 1ms = 1000ms

GOOD: batch into 1 query
  db.query("SELECT * FROM products WHERE id IN (?, ?, ..., ?)", item_ids)
  → 1 round trip × 5ms = 5ms (200x faster!)

Same pattern everywhere:
  Redis: MGET instead of 1000 GETs (pipeline)
  HTTP:  batch API (POST /api/batch with multiple operations)
  Kafka: batch messages into a single produce request
  DB:    bulk INSERT instead of 1000 individual INSERTs
```

### Connection Pooling

```
BAD: new connection per request
  Request → TCP handshake (1ms) → TLS handshake (2ms) → query (1ms) → close
  Overhead: 3ms per request just for connection setup!

GOOD: reuse connections from a pool
  Startup: create 20 connections, keep them in a pool
  Request → borrow connection from pool (0.01ms) → query (1ms) → return to pool
  Overhead: 0.01ms (300x less)

  Pool sizing: connections = (cores × 2) + effective_spindle_count
  For SSD with 8 cores: ~20 connections is usually optimal.
  More connections ≠ better (context switching overhead).
```

### Zero-Copy I/O

```
Normal file serving:
  Disk → kernel buffer → user buffer → kernel buffer → network socket
  4 memory copies + 2 context switches

sendfile() / splice():
  Disk → kernel buffer → network socket
  2 memory copies + 0 user-space copies

  Used by: Nginx (static files), Kafka (log segments)
  Savings: ~30% CPU reduction for I/O-heavy workloads
```

## 4. Serialization Optimization

```
Format comparison for a 1KB JSON object:

  JSON:     ~1000 bytes, parse: 5µs      (human-readable, universal)
  Protobuf: ~400 bytes,  parse: 0.5µs    (binary, schema-required)
  FlatBuf:  ~500 bytes,  parse: 0ns      (zero-copy, no parse needed!)
  MsgPack:  ~600 bytes,  parse: 1µs      (binary JSON, schema-optional)

  JSON → Protobuf: 2.5x smaller, 10x faster deserialization
  JSON → FlatBuffers: 2x smaller, zero deserialization cost

  Interview: "For internal service-to-service calls, I'd use Protobuf
  because it's 10x faster to parse and 2.5x smaller than JSON.
  For public APIs, I'd keep JSON for developer experience."
```

## 5. Database Query Optimization

```
1. Add an INDEX on columns you filter/sort by:
   SELECT * FROM orders WHERE user_id = 42 ORDER BY created_at
   → Index on (user_id, created_at) makes this instant

2. EXPLAIN ANALYZE every slow query:
   Shows: index scan vs sequential scan, rows examined, time per step

3. N+1 query problem:
   BAD:  fetch 100 users, then 100 queries for each user's orders
   GOOD: fetch 100 users + JOIN orders in 1 query (or batch)

4. Pagination:
   BAD:  OFFSET 10000 LIMIT 20 (DB scans 10020 rows, discards 10000)
   GOOD: WHERE id > last_seen_id LIMIT 20 (cursor-based, constant time)

5. Read replicas:
   Route read-heavy queries to replicas (80% of traffic).
   Writes still go to primary. 5x capacity with 5 replicas.

6. Denormalize for read performance:
   Pre-compute: likes_count column instead of COUNT(*) on every read.
   Trade write complexity for read speed.
```

## 6. Common Interview Performance Questions

| Question | Key Pattern |
|----------|------------|
| "Make this API 10x faster" | Cache (Redis), indexes, batch queries, connection pool |
| "Handle 100K RPS" | Horizontal scale + cache + read replicas + async processing |
| "Reduce memory usage" | Cache-friendly structs, arena allocator, object pools, compression |
| "Optimize hot loop" | SIMD, branchless code, cache-line alignment, avoid allocations |
| "Database is slow" | EXPLAIN query plan, add indexes, read replicas, denormalize |
| "Reduce latency P99" | Connection pool, async I/O, pre-warming cache, circuit breaker |
| "Handle large file uploads" | Stream (don't buffer), multipart, resume support, CDN |

## Summary: The Performance Toolkit

```
Layer           Technique                    Typical Improvement
─────────────────────────────────────────────────────────────
Memory          Cache-friendly layout         10-100x (vs pointer chasing)
Memory          Arena/pool allocator          5-10x (vs malloc)
Memory          Avoid false sharing           10-100x (multi-threaded)
Concurrency     Lock-free atomics             5-50x (vs mutex)
Concurrency     Sharding                      Nx (N = number of shards)
I/O             Batching                      100-1000x (vs individual ops)
I/O             Connection pooling            300x (vs open/close per req)
I/O             Zero-copy (sendfile)          1.3x (CPU savings)
Serialization   Protobuf over JSON            10x parse, 2.5x smaller
Database        Index on query columns        100-10000x (vs full scan)
Database        Read replicas                 Nx capacity (N = replicas)
Caching         Redis/Memcached               100x (1ms vs 100ms DB)
Architecture    Async processing              ∞ (don't block on slow work)
```
