# libmdbx (MDBX) Deep Dive

## Overview

libmdbx is an **embedded key-value store** based on memory-mapped B+ trees. It's a modernized fork of LMDB (Lightning Memory-Mapped Database) with fixes for LMDB's design limitations. Think of it as "LMDB done right" — same core idea (mmap a B+ tree), but with better write performance, crash safety, and operational behavior.

## History & Why It Exists

```
The lineage:

  BerkeleyDB (1990s):
    The original embedded key-value store. Complex, many features,
    many bugs. Oracle acquired it. License became restrictive.

  LMDB (2011, Howard Chu / Symas / OpenLDAP):
    Radical simplification: memory-map a B+ tree file directly.
    Readers see data via mmap — zero copy, zero serialization.
    Copy-on-write B+ tree: writers never modify existing pages.
    Used by: OpenLDAP, Monero, Caffe, Turbobadger.

    LMDB problems:
      - Fixed map size: must pre-declare max DB size at open time.
        Too small → DB full error. Too large → wastes address space.
      - No automatic compaction: deleted space not reclaimed unless
        you manually copy the database (mdb_copy).
      - Readers can block writers: long-lived read transaction
        prevents reuse of old pages → DB grows unbounded.
      - Page-level writes: even changing 1 byte rewrites a full 4 KB page.
      - Limited error handling and diagnostics.

  libmdbx (2015+, Leonid Yuriev / Positive Technologies / ReOpen):
    Fork of LMDB that fixes the above problems:
      ✓ Auto-growing database (no fixed map size)
      ✓ Automatic space reclamation (GC of freed pages)
      ✓ Reader-doesn't-block-writer improvements
      ✓ Better crash recovery and consistency checks
      ✓ Handle-stale-readers automatically
      ✓ Page-level write optimizations
      ✓ Richer API, better diagnostics

  Used by: Erigon (Ethereum client, THE primary use case driving adoption),
           Turso, various embedded systems, IoT.

  The Erigon connection:
    Erigon (formerly Turbo-Geth) chose libmdbx over LMDB and LevelDB
    for Ethereum blockchain storage because:
      - Ethereum needs ~2 TB of state data
      - Reads must be instant (mmap = zero-copy)
      - Writes must be transactional (ACID)
      - Database grows continuously (auto-resize needed)
      - LevelDB/RocksDB write amplification was too high
```

---

## 2. Core Design — Memory-Mapped B+ Tree

### How It Works

```
libmdbx maps the ENTIRE database file into virtual memory using mmap().
The database IS a B+ tree laid out in pages within this file.

  ┌──────────────────────────────────────────────────────────────┐
  │  Process address space                                        │
  │                                                               │
  │  0x7F000000: ┌─────────────────────────────────────────────┐ │
  │              │           mmap'd database file               │ │
  │              │                                               │ │
  │              │  Page 0: Meta page (root, txn ID, etc.)     │ │
  │              │  Page 1: Meta page (backup copy)            │ │
  │              │  Page 2: B+ tree internal node              │ │
  │              │  Page 3: B+ tree internal node              │ │
  │              │  Page 4: B+ tree leaf node (actual KV data) │ │
  │              │  Page 5: B+ tree leaf node                  │ │
  │              │  Page 6: Free/GC page list                  │ │
  │              │  ...                                         │ │
  │              │  Page N: ...                                 │ │
  │              └─────────────────────────────────────────────┘ │
  │                                                               │
  │  Readers: just dereference pointers into the mmap region.    │
  │  No read() syscall. No buffer copying. No deserialization.   │
  │  The B+ tree nodes ARE the data in memory. Zero-copy.        │
  │                                                               │
  │  OS page cache manages what's actually in RAM vs on disk.    │
  │  Hot pages stay in RAM. Cold pages are evicted by the OS.    │
  │  libmdbx doesn't manage its own buffer pool.                │
  └──────────────────────────────────────────────────────────────┘

Why mmap is fast for reads:

  Traditional DB (PostgreSQL, RocksDB):
    App: "give me key X"
    DB: find page → check buffer pool → if not cached: read() syscall
        → kernel copies disk → kernel buffer → DB buffer → return to app
    Multiple copies. Syscall overhead. Buffer pool management code.

  libmdbx:
    App: "give me key X"
    DB: find page → dereference pointer into mmap region → return pointer
    If page is in RAM (OS page cache): ~100 ns. No syscall. No copy.
    If page is NOT in RAM: page fault → OS loads from disk → then ~100 ns.

    The app gets a POINTER directly into the mmap'd B+ tree node.
    The returned data IS the on-disk format. No deserialization.
```

### Copy-on-Write (CoW) B+ Tree

```
libmdbx NEVER modifies existing pages in place.
Writers create NEW pages, then atomically switch the root pointer.

  Before write:
    Meta → Root (page 2) → Internal (page 3) → Leaf (page 4)
                                                  key=42, val="hello"

  Write: UPDATE key=42, val="world"

  Step 1: Allocate new leaf page (page 7)
    Copy page 4's content, modify key=42's value.
    Page 7: key=42, val="world"

  Step 2: Allocate new internal page (page 8)
    Copy page 3, update pointer from page 4 → page 7.

  Step 3: Allocate new root page (page 9)
    Copy page 2, update pointer from page 3 → page 8.

  Step 4: ATOMIC meta page update
    Meta page: root = page 9 (was page 2)
    This is ONE write to the meta page. Atomic on most filesystems.

  After write:
    Meta → Root (page 9) → Internal (page 8) → Leaf (page 7)
                                                  key=42, val="world"

    Old pages 2, 3, 4 are now FREE (added to free list / GC).
    Any READER that started before the write still sees pages 2→3→4.
    They see the OLD data. Consistent snapshot. No lock needed.

  ┌──────────────────────────────────────────────────────────────┐
  │  This is MVCC at the page level:                              │
  │                                                               │
  │  Writers: create new pages, swap root atomically.            │
  │  Readers: follow the root that was current when they started.│
  │  No reader locks. No writer blocks readers. MVCC via CoW.   │
  │                                                               │
  │  EXACTLY the same concept as:                                │
  │    - ZFS copy-on-write (filesystem level)                    │
  │    - Immutable persistent data structures (functional prog)  │
  │    - Git (each commit is a new tree root)                    │
  └──────────────────────────────────────────────────────────────┘
```

---

## 3. ACID Transactions

```
libmdbx provides full ACID:

  Atomicity:
    Write transaction modifies new pages in memory.
    Commit = update meta page (one atomic write).
    If crash before commit → old meta still points to old root → no change.
    If crash after commit → new meta points to new root → write is visible.
    No WAL needed! The CoW tree IS the recovery mechanism.

  Consistency:
    B+ tree invariants maintained by the write path.

  Isolation:
    READERS see a snapshot (the root at their txn start time).
    WRITERS see their own modifications + latest committed state.
    One writer at a time (serialized via mutex).
    Multiple concurrent readers (each sees its own snapshot).

  Durability:
    Commit calls msync() / fdatasync() on modified pages + meta page.
    After commit returns, data is on stable storage.

  ┌──────────────────────────────────────────────────────────────┐
  │  Key insight: NO WAL (Write-Ahead Log) needed.               │
  │                                                               │
  │  PostgreSQL: write to WAL → then write to data pages        │
  │              (two writes per transaction)                     │
  │                                                               │
  │  libmdbx: write new CoW pages → update meta page             │
  │           (one logical write path, crash-safe via CoW)       │
  │                                                               │
  │  Trade-off: CoW has write amplification (rewrite path from   │
  │  leaf to root), but avoids the WAL overhead entirely.        │
  └──────────────────────────────────────────────────────────────┘
```

---

## 4. Read Path — Why It's So Fast

```
Read transaction:

  1. Record current meta page's root pointer and txn ID.
     (This is the reader's "snapshot." No lock, just read a counter.)

  2. Walk the B+ tree from root to leaf:
     Dereference pointers directly in mmap'd memory.
     Each node is a page (4 KB). Binary search within the page.

  3. Return a POINTER to the value in the mmap region.
     The caller gets: { data: *const u8, len: usize }
     This points DIRECTLY into the mmap'd file.
     No copy. No allocation. No deserialization.
     The data is only valid for the lifetime of the read transaction.

  Performance:
    B+ tree depth for 1 billion keys: ~4-5 levels
    Each level: one page access (~100 ns if in RAM, ~100 µs if on SSD)
    Typical read (hot data): ~400-500 ns
    This is FASTER than RocksDB/LevelDB (which must check memtable +
    bloom filters + multiple SSTable levels).

  ┌──────────────────────────────────────────────────────────────┐
  │  Compare read paths:                                          │
  │                                                               │
  │  libmdbx: root → internal → internal → leaf → pointer        │
  │           ~4 pointer dereferences. ~400 ns. Zero-copy.       │
  │                                                               │
  │  RocksDB: check memtable → check L0 SSTables →              │
  │           bloom filter → check L1 → decompress block →       │
  │           binary search → copy value to user buffer           │
  │           ~5-50 µs. Multiple copies.                          │
  │                                                               │
  │  For pure READ performance, mmap'd B+ tree wins.            │
  └──────────────────────────────────────────────────────────────┘
```

---

## 5. Write Path — Copy-on-Write Details

```
Write transaction:

  1. Acquire write mutex (only ONE writer at a time).

  2. Read current root from meta page.

  3. For each PUT/DELETE:
     - Walk B+ tree to find target leaf.
     - Allocate NEW page (from free list or extend file).
     - Copy leaf, apply modification.
     - Walk UP to root, copying each ancestor page.
     - Update parent pointers to point to new child pages.

  4. Commit:
     a) msync/fdatasync all dirty (new) pages.
     b) Write new meta page with new root pointer + new txn ID.
     c) msync/fdatasync meta page.
     d) Release write mutex.
     e) Old pages become freeable (added to GC/free list).

  Write amplification:
    Changing 1 key = rewrite leaf + all ancestors to root.
    Tree depth 4 = 4 pages × 4 KB = 16 KB written for 1 key change.

    This is MORE write amplification than an LSM tree for random writes,
    but LESS total I/O than an LSM tree's compaction background work.

  Batching helps:
    If you PUT 1000 keys in one transaction, many will share
    ancestor pages. Total pages written << 1000 × 4.
    Always batch writes into larger transactions.

  Single writer:
    Only ONE write transaction can be active at a time.
    Multiple writes must be serialized.
    For high write throughput: batch into large transactions.
    This is the main limitation vs RocksDB (which can write concurrently
    to the memtable).
```

---

## 6. libmdbx vs LMDB — What's Different

```
┌──────────────────────┬──────────────────────┬──────────────────────┐
│                      │ LMDB                  │ libmdbx              │
├──────────────────────┼──────────────────────┼──────────────────────┤
│ Map size             │ FIXED at open time    │ Auto-grows           │
│                      │ (must guess max size) │ (resizes on demand)  │
│ Space reclamation    │ Manual (mdb_copy)     │ Automatic GC         │
│                      │                       │ (frees old pages)    │
│ Stale readers        │ Can block writers     │ Auto-detect + handle │
│                      │ indefinitely          │ stale reader cleanup │
│ Crash recovery       │ Basic                 │ More robust checks   │
│ Diagnostics          │ Minimal               │ Rich error info      │
│ API                  │ C only, minimal       │ C + C++, richer API  │
│ Write performance    │ Good                  │ Better (page reuse)  │
│ Read performance     │ Excellent             │ Excellent (same)     │
│ Maintenance          │ Mostly stable/frozen  │ Actively developed   │
│ License              │ OpenLDAP Public Lic.  │ Apache 2.0           │
└──────────────────────┴──────────────────────┴──────────────────────┘

The auto-growing map size alone was enough reason to switch.
In LMDB, you had to set mapsize=1TB upfront and hope you never
exceed it. libmdbx just grows the file as needed.
```

---

## 7. libmdbx vs RocksDB/LevelDB — B+ Tree vs LSM

```
┌──────────────────────┬──────────────────────┬──────────────────────┐
│                      │ libmdbx (B+ tree)     │ RocksDB (LSM tree)   │
├──────────────────────┼──────────────────────┼──────────────────────┤
│ Read latency         │ ~400 ns (mmap)        │ ~5-50 µs             │
│ Read amplification   │ 1 (direct page access)│ 1-5× (check levels)  │
│ Write latency        │ ~10-100 µs per txn    │ ~1-10 µs (memtable)  │
│ Write amplification  │ ~4× (CoW path)        │ ~10-30× (compaction) │
│ Space amplification  │ ~1× (CoW pages + GC)  │ ~1.1-1.5× (levels)   │
│ Concurrency (writes) │ Single writer          │ Concurrent writes    │
│ Concurrency (reads)  │ Unlimited concurrent  │ Unlimited concurrent │
│ Background I/O       │ None (no compaction)  │ Heavy (compaction)   │
│ Memory management    │ OS page cache (mmap)  │ Block cache + bloom  │
│ Recovery             │ Instant (CoW = crash  │ Replay WAL           │
│                      │ safe by design)       │                       │
│ Range scans          │ Excellent (sorted tree)│ Good (sorted runs)   │
│ Random writes        │ Moderate (CoW overhead)│ Excellent (append)   │
│ Sequential writes    │ Good (batched)        │ Excellent             │
│                      │                       │                       │
│ Best for             │ Read-heavy, embedded,  │ Write-heavy, large   │
│                      │ single-writer OK       │ data, need concurrency│
│                      │                       │                       │
│ Used by              │ Erigon, OpenLDAP      │ Everything (RocksDB  │
│                      │                       │ is everywhere)       │
└──────────────────────┴──────────────────────┴──────────────────────┘

Key tradeoff:
  libmdbx: reads are FASTER (mmap, no bloom filter checks, no level merging).
           writes are SIMPLER (no background compaction, no WAL).
           but: single writer, CoW write amplification for random updates.

  RocksDB: writes are FASTER for high-throughput random inserts.
           reads require more work (check memtable + bloom + levels).
           background compaction can cause latency spikes.
           more complex operationally (tuning compaction, bloom filters, etc.)
```

---

## 8. The mmap Controversy

```
mmap for database storage is controversial. Here's both sides:

PROS (why libmdbx uses mmap):
  + Zero-copy reads: return pointers directly into mapped pages.
  + No buffer pool code: OS page cache manages everything.
  + Simple implementation: no custom memory management.
  + Instant recovery: CoW means the file is always consistent.

CONS (why some DB engineers avoid mmap):
  - No control over eviction: OS decides what stays in RAM.
    A database knows its access patterns better than the OS.
  - Page faults are unpredictable: a "fast" pointer dereference
    can suddenly take 100 µs if the page was evicted.
  - TLB pressure: large mmap regions stress the TLB.
  - Cannot do async I/O easily: page faults block the thread.
  - Write-back is OS-controlled: you can't precisely control
    when dirty pages reach disk (msync is coarse).
  - 32-bit systems: can't mmap more than ~2 GB.

The Andy Pavlo paper ("Are You Sure You Want to Use MMAP in Your DBMS?", 2022)
argues against mmap for general-purpose databases. But for embedded KV stores
with read-heavy workloads, the simplicity and read speed are hard to beat.

libmdbx's position: "mmap is great for our use case (embedded, read-heavy,
single-writer). We wouldn't build PostgreSQL on mmap, but for a KV store
used by a blockchain client reading billions of keys, it's ideal."
```

---

## 9. Erigon / Ethereum Use Case

```
Why Erigon chose libmdbx for Ethereum state storage:

  Ethereum state: ~2 TB of key-value data (accounts, storage slots, code).
  Access pattern: mostly reads (serve RPC queries), batched writes (per block).

  Requirements:
    - Read billions of keys with sub-millisecond latency
    - ACID transactions (block processing must be all-or-nothing)
    - Database grows continuously (must auto-resize)
    - Minimal background I/O (no compaction storms)
    - Crash recovery must be instant (node restarts)

  Why NOT RocksDB:
    - Compaction storms caused latency spikes during block processing
    - Write amplification (10-30×) wore out SSDs faster
    - Complex tuning (dozens of knobs for compaction, bloom filters, etc.)
    - WAL replay on restart could take minutes

  Why libmdbx:
    - Reads are mmap'd pointers: ~400 ns per key lookup
    - No compaction: predictable latency
    - CoW: instant crash recovery, no WAL replay
    - Auto-growing: no need to pre-size for 2 TB
    - Single writer is fine: Ethereum processes one block at a time
    - Batch all state changes per block into one write transaction
```

---

## 10. Key API Concepts

```
// Open database (auto-sizing)
let env = Environment::open(path)?;

// Read transaction (zero-copy, multiple concurrent)
let txn = env.begin_ro_txn()?;
let db = txn.open_db(None)?;
let value: &[u8] = txn.get(db, b"key")?;
// `value` is a pointer into the mmap region!
// Valid only while `txn` is alive.
// DO NOT use after txn.commit() or drop.
txn.commit()?;

// Write transaction (single writer, batched)
let mut txn = env.begin_rw_txn()?;
let db = txn.open_db(None)?;
txn.put(db, b"key1", b"value1", WriteFlags::empty())?;
txn.put(db, b"key2", b"value2", WriteFlags::empty())?;
txn.put(db, b"key3", b"value3", WriteFlags::empty())?;
txn.commit()?;  // all 3 writes are atomic

// Named databases (sub-databases within one file)
let users_db = txn.open_db(Some("users"))?;
let orders_db = txn.open_db(Some("orders"))?;
// Each is a separate B+ tree within the same mmap'd file.

// Cursors (range scans)
let cursor = txn.cursor(db)?;
for (key, value) in cursor.iter_from(b"prefix:") {
    // Sorted iteration, very fast (sequential page access)
}
```

---

## 11. Key Numbers

```
Read latency (hot):      ~400-500 ns (mmap, in page cache)
Read latency (cold):     ~100 µs (page fault, SSD)
Write latency (commit):  ~100 µs - 1 ms (fdatasync)
Write amplification:     ~4× (CoW path, tree depth 4)
Max database size:       ~128 TB (limited by address space)
Max key size:            ~512 bytes (configurable)
Max value size:          ~2 GB
Page size:               4096 bytes (matches OS page size)
Concurrent readers:      unlimited
Concurrent writers:      1 (serialized)
Recovery time:           instant (no WAL replay)
File count:              1 data file + 1 lock file
```
