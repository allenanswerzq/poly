# Hashing — Comprehensive Guide

## Overview

Everything hash-related for system design and interviews — from the fundamentals
of hash functions to distributed hashing strategies, probabilistic structures,
spatial indexing, and integrity verification.

## Contents

| # | Module | What's Inside | Key Use Cases |
|---|--------|--------------|---------------|
| 1 | `hash_functions` | FNV-1a, DJB2, MurmurHash3, xxHash, MD5, SHA-1, SHA-256, CRC32 | Hash tables, integrity, security |
| 2 | `consistent_ring` | Consistent hash ring, virtual nodes, weighted | Distributed caches, database sharding |
| 3 | `rendezvous` | Rendezvous / Highest Random Weight hashing | CDNs, load balancers, simpler alternative to rings |
| 4 | `jump_hash` | Google's Jump Consistent Hash | Numbered partitions, minimal memory |
| 5 | `maglev` | Google's Maglev hashing (O(1) lookup table) | Network load balancers, connection-level hashing |
| 6 | `probabilistic` | HyperLogLog, Count-Min Sketch, MinHash | Cardinality estimation, frequency counting, similarity |
| 7 | `geohash` | Geohash encode/decode, neighbors, spatial index | Location-based services, proximity search |
| 8 | `merkle` | Merkle tree, content-addressable storage, hash chains | Git, blockchain, anti-entropy, deduplication |

---

## 1. Hash Function Fundamentals

A hash function maps arbitrary data → fixed-size number. Good hash functions need:
- **Deterministic**: same input always gives same output
- **Uniform**: outputs spread evenly (avoid clustering)
- **Avalanche**: flip 1 input bit → ~50% output bits change

There are two families: **non-cryptographic** (fast) and **cryptographic** (secure).

### Non-Cryptographic Hashes (fast, for hash tables & partitioning)

These prioritize speed. No security guarantees — you can reverse-engineer inputs.
Use for: hash tables, bloom filters, sharding keys, checksums.

```
"hello"  → 0x4f9f2cab   (FNV-1a, 32-bit)
"hello!" → 0xe1931d3e   (one char changed, completely different hash)
```

| Function | Bits | Speed | Quality | Used In |
|----------|------|-------|---------|---------|
| **FNV-1a** | 32/64 | Fast | Good | Rust's default HashMap, general purpose |
| **DJB2** | 32 | Very fast | OK | Simple hash tables, old systems |
| **MurmurHash3** | 32/128 | Fast | Excellent | Hadoop, Cassandra, Spark, bloom filters |
| **xxHash** | 32/64/128 | Fastest | Excellent | LZ4, Linux kernel, checksums |

### Cryptographic Hashes (slower, for security & integrity)

These are designed so you **cannot** find the input from the output (one-way),
and **cannot** find two different inputs producing the same output (collision-resistant).

```
"hello"  → 5d41402abc4b2a76b9719d911017c592                        (MD5, 128-bit)
"hello"  → aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d                (SHA-1, 160-bit)
"hello"  → 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e...     (SHA-256, 256-bit)
```

| Function | Bits | Status | Used In |
|----------|------|--------|---------|
| **MD5** | 128 | **BROKEN** (2004) | File checksums (non-security), cache keys |
| **SHA-1** | 160 | **BROKEN** (2017) | Git commit hashes (legacy), avoid for new use |
| **SHA-256** | 256 | **Secure** | TLS/SSL, Bitcoin, digital signatures, HMAC |
| **SHA-3 / BLAKE3** | 256+ | **Secure** | Modern alternative when you need the latest |
| **bcrypt / argon2** | varies | **Secure** | Password storage ONLY (intentionally slow) |

> **MD5 is "broken"** means attackers can craft two different files with the same MD5 hash.
> Safe for non-security uses (download checksums), never for passwords or signatures.
>
> **SHA-1 is "broken"** — Google demonstrated a collision in 2017 ("SHAttered" attack).
> Git still uses it but is migrating to SHA-256.

### CRC32 — NOT a hash function

CRC (Cyclic Redundancy Check) detects accidental bit-flips during transmission.
It's **not** a hash — poor distribution, trivially reversible, no collision resistance.
Used in: Ethernet, ZIP, PNG, gzip. Never use for hash tables or security.

### Speed comparison

Non-crypto hashes are **20-60x faster** than crypto hashes:

```
100K keys hashed:
  xxHash32:    ~3ms     (non-crypto)
  FNV-1a:     ~5ms     (non-crypto)
  MurmurHash3: ~5ms    (non-crypto)
  MD5:       ~110ms     (crypto, BROKEN)
  SHA-1:     ~220ms     (crypto, BROKEN)
  SHA-256:   ~320ms     (crypto, secure)
```

### Which hash to use?

```
Need to hash keys for a hash table?     → FNV-1a or MurmurHash3
Need fast checksums?                     → xxHash or CRC32
Need to partition/shard data?            → MurmurHash3 or consistent hashing
Need to verify file integrity?           → SHA-256
Need to sign something (TLS, JWT)?       → SHA-256 or SHA-3
Need to store passwords?                 → bcrypt or argon2 (NEVER MD5/SHA!)
Need content-addressing (Git, IPFS)?     → SHA-256
```

---

## 2. Consistent Hash Ring

**The problem**: Simple modulo hashing (`hash(key) % N`) remaps almost ALL keys when
you add/remove a server. With 3→4 servers, ~75% of keys move.

**Consistent hashing** maps both servers and keys onto a circle (0 to 2³²). Each key
walks clockwise to find its server. Adding/removing a server only moves ~K/N keys.

```
                    Node A
                      ●
                 ╱         ╲
            ╱    key "x"       ╲
       ●    (walks clockwise     ●
    Node D    to find Node A)  Node B
       ╲                         ╱
            ╲                 ╱
                      ●
                    Node C
```

**Virtual nodes** solve the uneven-distribution problem with few servers:
each physical server gets 100-200 positions on the ring spread evenly.

**When to use**: Redis Cluster, Memcached, DynamoDB, any distributed cache/DB sharding.

---

## 3. Rendezvous Hashing (HRW)

For each key, compute a score for EVERY server. Pick the highest score. That's it.

```
key "user:123":
  score(key, server-A) = 0.72
  score(key, server-B) = 0.91  ← highest, pick this
  score(key, server-C) = 0.45
```

**Why it's great**:
- No ring, no virtual nodes — dead simple
- Perfect uniform distribution by nature
- Adding/removing a server moves exactly K/N keys (mathematically optimal)

**Downside**: O(N) per lookup — must score all servers. Fine for N < ~1000.

**When to use**: When you want the simplest correct solution. GitHub's load balancer,
Microsoft CARP (Cache Array Routing Protocol), CDN origin selection.

---

## 4. Jump Consistent Hash (Google, 2014)

Extremely clever algorithm: maps a key to one of N buckets using only math.
**No data structures at all** — just a while loop with ~ln(N) iterations.

```rust
fn jump_hash(key: u64, num_buckets: u32) -> u32 {
    // ~5 lines of code, O(ln N), zero memory
}
```

**Properties**:
- Perfect balance: exactly 1/N keys per bucket
- Minimal disruption: N→N+1 moves exactly 1/(N+1) keys
- Zero memory overhead
- **Limitation**: buckets must be numbered 0..N, can only add/remove at the end

**When to use**: Internal partitioning where buckets are numbered (database shards,
Kafka partitions). Not suitable if you need named servers or arbitrary removal.

---

## 5. Maglev Hashing (Google, 2016)

Google's solution for their network load balancer. Builds a fixed-size lookup table
(typically 65,537 entries) at setup time. Each connection does a single array lookup.

```
Setup:                              Runtime:
┌───────────────┐                   hash("10.0.0.5:443") % 65537 = 42103
│ Build 65537   │                   table[42103] = "backend-3"
│ entry table   │                   → route to backend-3
│ (one time)    │
└───────────────┘                   O(1) per packet!
```

**Why it exists**: When you're processing millions of packets per second, even
O(log N) is too slow. Maglev gives O(1) with near-perfect balance and minimal
disruption when backends change.

**When to use**: L4 load balancers, network-level routing. Envoy proxy, Cilium,
Cloudflare use Maglev or similar. Overkill for application-level routing.

---

## 6. Probabilistic Hash Structures

These trade a small error margin for MASSIVE memory savings.

### HyperLogLog — "How many distinct users visited today?"

Counts unique items using only **16 KB of memory** regardless of cardinality.
1 billion distinct users? Still 16 KB. Error: ~0.8%.

```
Exact counting:    HashMap<UserId, ()>  → 8 GB for 1B users
HyperLogLog:       16 KB                → same answer ± 0.8%
```

**How it works**: Hash each item. Count the longest run of leading zeros seen.
More zeros → more items (probabilistic argument). Multiple registers reduce variance.

**Used in**: Redis `PFCOUNT`, database query optimizers, analytics dashboards.

### Count-Min Sketch — "How many times did user X appear?"

Estimates frequency of items in a stream. Uses a 2D array of counters.
Each item hashes to one cell per row; query returns the **minimum** across rows.

```
stream: A A B A C B A A
query("A") → 5 (true: 5)   ✓ always ≥ true count, never under-counts
query("D") → 0 or small    (false positives possible, false negatives never)
```

**Used in**: Network monitoring (heavy hitters), NLP word frequencies, database stats.

### MinHash — "How similar are these two documents?"

Estimates Jaccard similarity `|A∩B| / |A∪B|` using compact signatures.
Hash each item in a set, keep the K smallest hashes. Compare signatures.

```
Doc A and Doc B share 70% of their shingles
MinHash signature comparison → ~0.70  (using only 200 numbers per doc)
```

**Used in**: Google/Bing duplicate web page detection, plagiarism detection,
recommendation systems ("users who liked similar items").

---

## 7. Geohashing

Encodes (latitude, longitude) into a short string. **Nearby points share a common prefix**.
This turns "find things near me" into a simple string prefix query.

```
Statue of Liberty:  40.6892, -74.0445  →  "dr5r7p4"
Nearby point:       40.6895, -74.0440  →  "dr5r7p6"  (6 chars in common!)
Eiffel Tower:       48.8584,   2.2945  →  "u09tunq"  (0 chars in common)
```

Precision levels:
| Chars | Cell Size | Example Use |
|-------|-----------|-------------|
| 5 | ~4.9 km | City-level search |
| 7 | ~153 m | Street-level search |
| 9 | ~4.8 m | Building-level |

**How to query "nearby"**: compute the geohash of the query point + its 8 neighbor
cells. Look up all items in those 9 cells. Filter by actual distance.

**Used in**: Uber (find nearby drivers), DoorDash, Yelp, Redis GEO, Elasticsearch.

---

## 8. Merkle Tree & Content-Addressable Storage

### Merkle Tree — "Which data blocks differ between two replicas?"

A binary tree where each leaf is a hash of a data block, and each internal node
is the hash of its two children. The root hash is a fingerprint of ALL data.

```
          root: H(H01 + H23)         ← change ANY block, root changes
         ╱                    ╲
    H01: H(H0+H1)        H23: H(H2+H3)
    ╱        ╲            ╱        ╲
  H0          H1        H2          H3
  │           │         │           │
Block-0    Block-1   Block-2    Block-3
```

**Key insight**: Two nodes compare root hashes. If equal → data is identical (done!).
If different → recurse into subtrees to find WHICH blocks differ. Syncs O(log N) hashes
instead of all data.

**Used in**: Git (every commit is a Merkle root), Bitcoin/Ethereum (transaction trees),
Amazon DynamoDB & Cassandra (anti-entropy repair), IPFS, Docker image layers.

### Content-Addressable Storage — "Store by hash, deduplicate for free"

Store data by its hash. Same content → same key → no duplicates.

```
put("hello world") → "4eccf346..."   // SHA-256 of content
put("hello world") → "4eccf346..."   // same content = same hash = already stored
get("4eccf346...") → "hello world"
```

**Used in**: Git objects, IPFS blocks, Docker layers, backup systems (Borg, Restic).

---

## When to Use What

```
Need to distribute keys across servers?
├─ Servers change frequently → Consistent Hash Ring (virtual nodes)
├─ Simple setup, < 1000 nodes → Rendezvous Hashing
├─ Sequential bucket numbers → Jump Consistent Hash
└─ Network load balancer, O(1) → Maglev Hashing

Need approximate counting?
├─ How many distinct users? → HyperLogLog
├─ How often does X appear? → Count-Min Sketch
└─ How similar are two sets? → MinHash / LSH

Need spatial queries?
└─ "Find nearby X" → Geohashing

Need data integrity / dedup?
├─ Verify data hasn't changed → Merkle Tree
├─ Deduplicate storage → Content-Addressable Storage
└─ Tamper-proof chain → Hash Chain
```

## Complexity Comparison

```
┌────────────────────┬──────────┬──────────┬──────────┬──────────────┐
│ Algorithm          │ Lookup   │ Add Node │ Memory   │ Balance      │
├────────────────────┼──────────┼──────────┼──────────┼──────────────┤
│ hash(k) % N        │ O(1)     │ O(K)     │ O(1)     │ Perfect      │
│ Consistent Ring    │ O(log V) │ O(V)     │ O(N·V)   │ Good w/ vnodes│
│ Rendezvous (HRW)  │ O(N)     │ O(1)     │ O(N)     │ Perfect      │
│ Jump Hash         │ O(ln N)  │ O(1)     │ O(1)     │ Perfect      │
│ Maglev            │ O(1)     │ O(M·N)   │ O(M)     │ Near-perfect │
└────────────────────┴──────────┴──────────┴──────────┴──────────────┘
N = nodes, V = virtual nodes per node, K = total keys, M = table size
```

## Interview Tips

1. **Draw the ring** — visualize hash space as a circle
2. **Explain virtual nodes** — solves uneven distribution with few servers
3. **Know the math**: adding 1 node to N remaps only 1/(N+1) keys
4. **Mention replication**: store on N consecutive nodes on the ring
5. **Know alternatives**: Rendezvous (simpler, no ring), Jump (zero memory), Maglev (O(1) lookup)
6. **HyperLogLog**: "count distinct users with 16KB memory and <1% error"
7. **Geohash**: "nearby queries by comparing string prefixes"
8. **Merkle tree**: "efficiently find which blocks differ between replicas"
9. **Content-addressable**: "store by hash → dedup is free" (Git, Docker, IPFS)

## Run

```bash
cargo run --bin consistent-hashing
```
