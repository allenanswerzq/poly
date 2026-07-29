# Performance & Memory Optimization — What Must Know

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

  JSON:       ~1000 bytes, parse: 5µs      (human-readable, universal)
  MsgPack:    ~600 bytes,  parse: 1µs      (binary JSON, schema-optional)
  Protobuf:   ~400 bytes,  parse: 0.5µs    (binary, schema-required)
  FlatBuf:    ~500 bytes,  parse: 0ns      (zero-copy, no parse needed!)
  Cap'n Proto: ~450 bytes, parse: 0ns      (zero-copy, like FlatBuf)

  JSON → Protobuf: 2.5x smaller, 10x faster deserialization
  JSON → FlatBuffers: 2x smaller, zero deserialization cost

  Interview: "For internal service-to-service calls, I'd use Protobuf
  because it's 10x faster to parse and 2.5x smaller than JSON.
  For public APIs, I'd keep JSON for developer experience."
```

### Cap'n Proto (Kenton Varda, 2013 — by the author of Protobuf v2)

```
Cap'n Proto is like FlatBuffers — zero-copy, no parsing step.
The wire format IS the in-memory format. Created by the engineer
who designed Protobuf v2 at Google, then left and said
"what if we just didn't have the parse step at all?"

How it works (same idea as FlatBuffers):
  Serialized bytes = the data structure itself.
  "Deserializing" = cast the byte pointer. No allocation, no copy.

  ┌──────────────────────────────────────────────────────────────┐
  │  Traditional (JSON, Protobuf):                                │
  │    receive bytes → PARSE → build objects in memory → use      │
  │    (allocations, copies, CPU work)                            │
  │                                                               │
  │  Cap'n Proto / FlatBuffers:                                   │
  │    receive bytes → DONE. Read directly from the buffer.       │
  │    (zero allocation, zero copy, pointer arithmetic only)      │
  └──────────────────────────────────────────────────────────────┘

  # Schema (like protobuf .proto)
  struct Person {
    name @0 :Text;
    age  @1 :UInt32;
    email @2 :Text;
  }

  # Reading — zero parse
  let reader = message.get_root::<person::Reader>()?;
  println!("{}", reader.get_name()?);   // reads from the byte buffer directly
  println!("{}", reader.get_age());     // no deserialization happened
```

### Cap'n Proto vs FlatBuffers — What's Different?

```
┌──────────────────────┬──────────────────────┬──────────────────────┐
│                      │ FlatBuffers          │ Cap'n Proto          │
├──────────────────────┼──────────────────────┼──────────────────────┤
│ Creator              │ Google (2014)        │ Kenton Varda (2013)  │
│                      │                      │ (ex-Google, wrote    │
│                      │                      │  Protobuf v2)        │
│ Parse time           │ ~0 ns                │ ~0 ns                │
│ Zero-copy            │ Yes                  │ Yes                  │
│ Mutable after create │ No (read-only)       │ Yes (read + write)   │
│ RPC framework        │ No (just serializer) │ Yes (built-in RPC)   │
│ Time-travel / pipelining│ No               │ Yes (promise pipeline)│
│ Canonical encoding   │ No                   │ Yes (bit-for-bit same│
│                      │                      │  output every time)  │
│ Sandboxing support   │ No                   │ Yes (capability-based│
│                      │                      │  security model)     │
│ Language support     │ Broad (C++, Rust,    │ C++, Rust, Go, JS,   │
│                      │ Java, JS, Python...) │ Python               │
│ Used by              │ Google, Netflix,     │ Cloudflare, Sandstorm│
│                      │ Facebook             │ Waymo                │
└──────────────────────┴──────────────────────┴──────────────────────┘

Cap'n Proto's unique features:

  1. PROMISE PIPELINING (RPC)
     Traditional RPC:
       result1 = call_server_A()       // wait for response
       result2 = call_server_B(result1) // wait again — 2 round trips

     Cap'n Proto RPC:
       promise1 = call_server_A()          // don't wait
       promise2 = call_server_B(promise1)  // send immediately, server
                                           // will chain them
       result = promise2.wait()            // 1 round trip total!

     The server knows result2 depends on result1 and pipelines them.

  2. CAPABILITY-BASED SECURITY
     Instead of "can user X do action Y?" (ACL),
     Cap'n Proto passes capabilities (object references) around.
     If you have a reference to an object, you can use it.
     If you don't, you can't. No ambient authority.
     Used by Cloudflare Workers for sandboxing.

  3. CANONICAL ENCODING
     Same data always produces the exact same bytes.
     Useful for hashing, caching, deduplication.
     Protobuf/FlatBuffers don't guarantee this.
```

### The Full Serialization Comparison

```
┌──────────────┬──────┬────────┬─────────┬──────────┬──────────┬───────────┐
│              │ JSON │ MsgPack│ Protobuf│ FlatBuf  │Cap'n Proto│ Avro      │
├──────────────┼──────┼────────┼─────────┼──────────┼──────────┼───────────┤
│ Format       │ Text │ Binary │ Binary  │ Binary   │ Binary   │ Binary    │
│ Schema       │ No   │ No     │ Required│ Required │ Required │Required  │
│ Human read   │ Yes  │ No     │ No      │ No       │ No       │  No        │
│ Self-descr.  │ Yes  │ Yes    │ No      │ No       │ No       │ Yes(header│
│ Zero-copy    │ No   │ No     │ No      │ YES      │ YES      │ No        │
│ Size         │ 100% │ ~60%   │ ~40%    │ ~50%     │ ~45%     │ ~40%      │
│ Parse speed  │ Slow │ Fast   │ Faster  │ ~0 ns    │ ~0 ns    │ Fast      │
│ Mutable      │ Yes  │ Yes    │ Yes     │ No       │ Yes      │ Yes       │
│ Built-in RPC │ No   │ No     │ gRPC    │ No       │ Yes      │ No        │
│ Best for     │ APIs │ Drop-in│ gRPC,   │ Games,   │ RPC,     │ Hadoop,   │
│              │ debug│ for JSON│ micro-  │ HFT,     │ sandbox, │ Kafka,    │
│              │      │        │ services│ real-time│ Cloudflare│ Spark     │
└──────────────┴──────┴────────┴─────────┴──────────┴──────────┴───────────┘

Decision guide:
  Need debugging / public API?          → JSON
  Need faster JSON, no schema change?   → MessagePack
  Need typed APIs / gRPC?               → Protobuf
  Need zero-copy, read-only access?     → FlatBuffers
  Need zero-copy + RPC + security?      → Cap'n Proto
  Need Hadoop/Kafka/Spark ecosystem?    → Avro
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

## 6. Common Encoding Methods

Encoding transforms data between representations. Different from serialization
(which specifically converts data structures to bytes).

### Text Encoding — How Characters Become Bytes

```
Computers store bytes (0-255). Characters need a mapping to bytes.

ASCII (1963):
  'A' = 65, 'B' = 66, ..., 'z' = 122
  128 characters total (7 bits). English only. No é, no 中, no 😀.

Latin-1 / ISO-8859-1:
  Extends ASCII to 256 characters. Adds accented chars (é, ñ, ü).
  Still no Chinese, Japanese, Arabic, emoji.

UTF-8 (1993, now dominant):
  Variable-length encoding for ALL Unicode characters (150,000+).
  Backwards-compatible with ASCII.

  'A'    → 0x41                    (1 byte, same as ASCII)
  'é'    → 0xC3 0xA9              (2 bytes)
  '中'   → 0xE4 0xB8 0xAD         (3 bytes)
  '😀'   → 0xF0 0x9F 0x98 0x80    (4 bytes)

  Why UTF-8 won:
    • ASCII text is valid UTF-8 (zero migration cost)
    • No byte-order issues (unlike UTF-16/32)
    • Compact for English/Latin text (1 byte per char)
    • Self-synchronizing (can find char boundaries in any position)

  ┌─────────────────────────────────────────────────────────────┐
  │ UTF-8 byte patterns:                                        │
  │                                                             │
  │  0xxxxxxx                          → 1 byte  (ASCII, 0-127)│
  │  110xxxxx 10xxxxxx                 → 2 bytes (128-2047)    │
  │  1110xxxx 10xxxxxx 10xxxxxx        → 3 bytes (2048-65535)  │
  │  11110xxx 10xxxxxx 10xxxxxx 10xxxxxx → 4 bytes (65536+)    │
  │                                                             │
  │  Leading byte tells you how many bytes the character uses.  │
  │  Continuation bytes always start with 10.                   │
  │  This means you can jump to any byte and find the next      │
  │  character boundary — no scanning from the start.           │
  └─────────────────────────────────────────────────────────────┘

UTF-16 (Java, JavaScript, Windows):
  2 bytes for most chars, 4 bytes for rare ones (surrogate pairs).
  '中' = 0x4E2D (2 bytes). '😀' = 0xD83D 0xDE00 (4 bytes — surrogate pair).
  Problem: byte order matters (big-endian vs little-endian → BOM needed).
  Problem: NOT compatible with ASCII (breaks C string functions).

UTF-32:
  Fixed 4 bytes per character. Simple but wasteful. Rarely used for storage.
  Useful for internal processing (constant-time indexing by char position).

  ┌─────────────┬─────────┬──────────────────┬──────────────────┐
  │ Encoding    │"Hello"  │ "中文"            │ "Hello中文"       │
  ├─────────────┼─────────┼──────────────────┼──────────────────┤
  │ UTF-8       │ 5 bytes │ 6 bytes          │ 11 bytes         │
  │ UTF-16      │10 bytes │ 4 bytes          │ 14 bytes         │
  │ UTF-32      │20 bytes │ 8 bytes          │ 28 bytes         │
  └─────────────┴─────────┴──────────────────┴──────────────────┘

  UTF-8 wins for English-heavy text. UTF-16 wins for CJK-heavy text.
  But UTF-8 is the universal standard for files, networks, and APIs.
```

### Binary-to-Text Encoding — Making Bytes Safe for Text

```
Problem: you have raw binary data (image, hash, key) but need to
put it in a text context (JSON, URL, email, HTML attribute).
Raw bytes can contain nulls, control chars, non-printable — breaks text.

Base64:
  Encodes 3 bytes → 4 ASCII characters (A-Z, a-z, 0-9, +, /)
  Overhead: 33% larger (3 bytes become 4 characters).

  Binary:  0xFF 0x00 0xAB                (3 bytes)
  Base64:  "/wCr"                         (4 chars)

  Used everywhere:
    • JWT tokens: header.payload.signature (all base64)
    • Data URIs: <img src="data:image/png;base64,iVBOR...">
    • Email attachments (MIME encoding)
    • Embedding binary in JSON: {"image": "iVBORw0KGgo..."}
    • HTTP Basic Auth: Authorization: Basic dXNlcjpwYXNz

  Variants:
    Standard:    A-Za-z0-9+/  with = padding
    URL-safe:    A-Za-z0-9-_  (replaces +/ which have meaning in URLs)
    No-pad:      omit trailing = (when length is known)

  In Rust:
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let decoded = base64::engine::general_purpose::STANDARD.decode(&encoded)?;

Hex encoding:
  Each byte → 2 hex characters (0-9, a-f).
  Overhead: 100% larger (1 byte = 2 chars). But very readable.

  Binary:  0xFF 0x00 0xAB
  Hex:     "ff00ab"

  Used in:
    • Hash outputs: SHA-256 → "e3b0c44298fc1c149afb..."  (64 hex chars)
    • Color codes: #FF5733
    • MAC addresses: AA:BB:CC:DD:EE:FF
    • Debugging (hexdump)

  ┌───────────────┬───────────┬─────────────────────────────┐
  │ Encoding      │ Overhead  │ Use case                    │
  ├───────────────┼───────────┼─────────────────────────────┤
  │ Base64        │ +33%      │ Embedding binary in text    │
  │ Base64-URL    │ +33%      │ JWT, URL-safe binary        │
  │ Hex           │ +100%     │ Hashes, debugging, colors   │
  │ Base32        │ +60%      │ Case-insensitive contexts   │
  │ Base58        │ +37%      │ Bitcoin addresses (no 0OIl) │
  │ Base85/Ascii85│ +25%      │ PDF, git binary patches     │
  └───────────────┴───────────┴─────────────────────────────┘
```

### URL Encoding (Percent Encoding)

```
URLs can only contain certain characters. Others must be escaped.

  "hello world"     → "hello%20world"       (%20 = space)
  "price=10&qty=5"  → "price%3D10%26qty%3D5" (%3D = '=', %26 = '&')
  "café"            → "caf%C3%A9"           (%C3%A9 = UTF-8 bytes of 'é')

  Rule: unsafe character → %XX where XX is the hex byte value.
  Safe characters: A-Z a-z 0-9 - _ . ~

  In Rust:
    use urlencoding::encode;
    let encoded = encode("hello world");  // "hello%20world"
```

### Compression Encoding — Make Data Smaller

```
Lossless (exact reconstruction):

  ┌──────────────┬──────────┬─────────┬──────────────────────────────┐
  │ Algorithm    │ Ratio    │ Speed   │ Used in                      │
  ├──────────────┼──────────┼─────────┼──────────────────────────────┤
  │ gzip/deflate │ 3-5x     │ Medium  │ HTTP (Content-Encoding: gzip)│
  │ zstd         │ 3-5x     │ Fast    │ Kafka, HTTP, databases, tar  │
  │ lz4          │ 2-3x     │ Fastest │ Real-time, databases, logs   │
  │ brotli       │ 4-6x     │ Slow    │ Web (static assets, CDN)     │
  │ snappy       │ 2-3x     │ Fast    │ Google (Bigtable, RPC)       │
  │ zlib         │ 3-5x     │ Medium  │ PNG, ZIP, PDF                │
  └──────────────┴──────────┴─────────┴──────────────────────────────┘

  Tradeoff: better compression ratio ↔ slower speed
    lz4:    "I need speed, compression ratio doesn't matter much"
    zstd:   "I want good compression AND good speed" (best default choice)
    brotli: "I'll compress once, decompress many times" (static web assets)
    gzip:   "I need universal compatibility" (every HTTP client supports it)

  Zstandard (zstd) is the modern default:
    Created by Yann Collet (Facebook, 2016, also created lz4).
    Adjustable levels (1-22): level 1 ≈ lz4 speed, level 19 ≈ gzip ratio.
    Dictionary mode: train on similar data for 2-5x better compression.
    Used by: Kafka, Linux kernel, tar, HTTP, databases.

Lossy (approximate, smaller):
  JPEG:  images (lose invisible detail, 10-20x compression)
  MP3:   audio (lose inaudible frequencies)
  H.264: video (lose imperceptible motion detail)
  Quantization: ML weights FP32 → INT8 (lose precision, 4x smaller)
```

### HTML Entity Encoding

```
In HTML, < > & " have special meaning. Must escape them in content.

  <script>alert("xss")</script>
  → &lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;

  This PREVENTS XSS (cross-site scripting) attacks.
  If user input contains <script>, it renders as text, not executed.

  Common entities:
    <  → &lt;
    >  → &gt;
    &  → &amp;
    "  → &quot;
    '  → &#39;
```

### When to Use What

```
Need to store/send a data structure?          → Serialization (JSON, Protobuf)
Need to embed binary in text (JSON, URL)?     → Base64
Need to display a hash?                       → Hex
Need to put user input in a URL?              → URL encoding (percent encoding)
Need to show user input in HTML?              → HTML entity encoding
Need to make HTTP responses smaller?          → gzip or zstd (Content-Encoding)
Need to store text in any language?           → UTF-8 (always)
Need to make static web assets smaller?       → Brotli (best ratio, slow compress OK)
Need fast compression for real-time data?     → lz4 or zstd level 1
```

## 7. Common Interview Performance Questions

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

## Compute-Bound vs Memory-Bound — General Optimization

Every performance problem falls into one of two categories:
**compute-bound** (CPU can't crunch numbers fast enough) or
**memory-bound** (CPU is stalled waiting for data from RAM/cache).

### Compute-Bound (CPU is the bottleneck)

```
Symptom: CPU at 100%, memory bandwidth not saturated.
Example: compression, encryption, physics simulation, image rendering, regex.

Optimize by doing LESS work or FASTER work:

  1. Better algorithm       O(N²) → O(N log N)
     The single biggest win. Always check this first.
     Example: bubble sort → quicksort = 1000x faster for 100K elements.

  2. SIMD (vectorization)   process 8 floats per instruction (AVX2)
     Let the CPU do 4-16 operations in one instruction.
     Compilers often auto-vectorize if your loop is simple enough.
     Rust: enable target-cpu=native for auto-vectorization.

  3. Reduce precision       f64 → f32 if acceptable
     Half the bytes = twice the SIMD width = 2x throughput.
     ML training switched from FP32 → BF16 for exactly this reason.

  4. Parallelize            split across cores (rayon, OpenMP, tokio)
     N cores = up to Nx speedup (Amdahl's law limits this).
     Rayon in Rust: .par_iter() — often a one-line change.

  5. Avoid branches         branchless code, lookup tables
     Branch mispredictions cost ~15 cycles each.
     Replace if/else with arithmetic: result = a * cond + b * (1 - cond)

  6. Compiler optimization  -O3, PGO (profile-guided optimization), LTO
     PGO: compile → profile on real workload → recompile with profile data.
     Gives 10-30% speedup by optimizing hot paths and inlining decisions.

  7. Hot loop optimization  unroll, minimize function calls
     Keep the inner loop tight: no allocations, no virtual dispatch,
     no unnecessary bounds checks.
```

### Memory-Bound (RAM/cache is the bottleneck)

```
Symptom: CPU often idle/stalled, memory bandwidth near peak.
Example: database scans, large array traversal, graph algorithms,
         hash table lookups, LLM inference, analytics queries.

Optimize by moving LESS data or moving it FASTER:

  1. Smaller data types     i64 → i32, String → &str, struct packing
     Smaller data = more fits in cache = fewer cache misses.
     A Vec<u32> is 2x more cache-friendly than Vec<u64>.

  2. Cache-friendly layout  Array-of-Structs → Struct-of-Arrays
     If you only access one field, don't load the whole struct.

     // BAD: loads 120 bytes per user, uses 4 bytes (age)
     struct User { name: String, email: String, age: u32, bio: String }
     users.iter().map(|u| u.age).sum()

     // GOOD: loads ONLY ages, 4 bytes each, cache lines fully used
     struct Users { names: Vec<String>, ages: Vec<u32>, ... }
     users.ages.iter().sum()

     This is why columnar databases (DuckDB, ClickHouse) are fast for analytics.
     They store each column contiguously — only read the columns you need.

  3. Sequential access      iterate linearly, avoid random jumps
     CPUs prefetch sequential memory automatically (~20 cache lines ahead).
     Random access kills this: every access is a cache miss.

     // Fast: sequential scan, CPU prefetcher active
     for item in &data { sum += item; }

     // Slow: random access, every read is a cache miss
     for &idx in &random_indices { sum += data[idx]; }

  4. Avoid pointer chasing  Vec<T> instead of LinkedList<T>
     Linked list: each node is a separate allocation, random in memory.
     Vec: contiguous memory, CPU prefetcher loves it.
     HashMap is also pointer-chasey → consider perfect hashing or arrays.

  5. Batch processing       process data in cache-sized chunks
     Process data in blocks that fit in L2/L3 cache.
     Tiled/blocked algorithms: matrix multiply, database joins.

  6. Reduce allocations     reuse buffers, arena allocators
     malloc/free involve syscalls and can fragment memory.
     Arena: allocate a big chunk once, bump-allocate within it.
     Reset the whole arena when done — no individual frees.

  7. Prefetching            manual hints or compiler-assisted
     Tell the CPU to start loading data you'll need soon.
     C/Rust: _mm_prefetch / core::arch::x86_64::_mm_prefetch
     Usually the compiler/hardware handles this well for sequential access.
```
