# FoundationDB — The Database That Databases Are Built On

---

## 1. What FoundationDB Is and Why It Matters

```
FoundationDB is a distributed, ordered key-value store with
FULL ACID TRANSACTIONS across multiple keys.

That last part is the key differentiator. Most distributed KV stores
give you either:
  - Strong consistency but NO multi-key transactions (etcd)
  - Multi-key operations but WEAK consistency (Cassandra)
  - ACID transactions but limited distribution (single-node Postgres)

FoundationDB gives you ALL THREE: distributed + strongly consistent
+ serializable ACID transactions. Then it says: "build whatever
you want on top of this."

The philosophy:
  FoundationDB is NOT a database you use directly.
  It's a DATABASE ENGINE that other databases are built on.

  ┌──────────────────────────────────────────────────────────────┐
  │  "Layers" built on FoundationDB:                            │
  │                                                              │
  │  ┌────────────┐ ┌────────────┐ ┌──────────┐ ┌───────────┐ │
  │  │ Document   │ │  Graph     │ │  SQL     │ │  Queue    │ │
  │  │ Store      │ │  Database  │ │(CQL/SQL) │ │           │ │
  │  └─────┬──────┘ └─────┬──────┘ └────┬─────┘ └─────┬─────┘ │
  │        │              │             │             │        │
  │        └──────────────┴──────┬──────┴─────────────┘        │
  │                              │                              │
  │                    FoundationDB KV Store                    │
  │              (ordered keys, ACID transactions)              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Apple runs FoundationDB as the backbone of iCloud (CloudKit).
  Snowflake uses it for metadata storage.
  Tigris Data uses it for their database service.

History:
  2009  Founded by Dave Scherer and Dave Rosenthal
  2012  FoundationDB 1.0 released
  2015  Apple acquires FoundationDB (goes closed-source)
  2018  Apple open-sources FoundationDB (Apache 2.0)
  2019  FoundationDB Record Layer (Apple's structured layer) open-sourced
  2024  Active development continues, used at Apple's full scale

The FoundationDB paper (SIGMOD 2021) is one of the best real-world
distributed systems papers ever written. Read it.
```

---

## 2. Core Architecture

```
FoundationDB separates three concerns into separate subsystems:

  ┌──────────────────────────────────────────────────────────────────┐
  │                   FoundationDB Architecture                      │
  │                                                                  │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │              COORDINATION LAYER                          │   │
  │  │  (Coordinators — 3 or 5 nodes)                          │   │
  │  │  Stores cluster metadata: who is the transaction system,│   │
  │  │  who is the storage system. Rarely changes.             │   │
  │  └──────────────────────────────────────────────────────────┘   │
  │          │                                                       │
  │          ▼                                                       │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │              TRANSACTION SYSTEM                          │   │
  │  │                                                          │   │
  │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │   │
  │  │  │ Proxy       │  │ Proxy       │  │ Proxy       │    │   │
  │  │  │ (client     │  │             │  │             │    │   │
  │  │  │  interface) │  │             │  │             │    │   │
  │  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘    │   │
  │  │         │                │                │            │   │
  │  │         ▼                ▼                ▼            │   │
  │  │  ┌─────────────┐  ┌──────────────────────────────┐    │   │
  │  │  │ Sequencer   │  │ Resolvers                     │    │   │
  │  │  │ (assigns    │  │ (detect read-write conflicts  │    │   │
  │  │  │  versions)  │  │  between concurrent txns)     │    │   │
  │  │  └─────────────┘  └──────────────────────────────┘    │   │
  │  │         │                                              │   │
  │  │         ▼                                              │   │
  │  │  ┌──────────────────────────────────────────────┐     │   │
  │  │  │ Log Servers (transaction log)                 │     │   │
  │  │  │ Write-ahead log for COMMITTED transactions.   │     │   │
  │  │  │ Data goes here FIRST, acknowledged to client, │     │   │
  │  │  │ then asynchronously pushed to storage servers. │     │   │
  │  │  └──────────────────────────────────────────────┘     │   │
  │  └──────────────────────────────────────────────────────────┘   │
  │          │ (async replication)                                   │
  │          ▼                                                       │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │              STORAGE SYSTEM                              │   │
  │  │                                                          │   │
  │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │   │
  │  │  │ Storage     │  │ Storage     │  │ Storage     │    │   │
  │  │  │ Server 1    │  │ Server 2    │  │ Server 3    │    │   │
  │  │  │ [aaa-fff]   │  │ [fff-nnn]   │  │ [nnn-zzz]   │    │   │
  │  │  │             │  │             │  │             │    │   │
  │  │  │ SQLite +    │  │ SQLite +    │  │ SQLite +    │    │   │
  │  │  │ B-tree      │  │ B-tree      │  │ B-tree      │    │   │
  │  │  └─────────────┘  └─────────────┘  └─────────────┘    │   │
  │  │                                                          │   │
  │  │  Each storage server holds a RANGE of the key space.    │   │
  │  │  Data is replicated (3 copies by default).              │   │
  │  │  Storage servers pull committed data from log servers.  │   │
  │  └──────────────────────────────────────────────────────────┘   │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

  KEY INSIGHT: Transaction processing and storage are DECOUPLED.
  The transaction system (log servers) can be small and fast.
  The storage system can be large and scale independently.
  This is why FoundationDB scales so well.
```

---

## 3. How Transactions Work — The Core Magic

FoundationDB uses **Optimistic Concurrency Control (OCC)** with
**Multi-Version Concurrency Control (MVCC)**.

### 3.1 Transaction Lifecycle

```
Client code (pseudocode):
  tr = db.create_transaction()
  balance = tr.get("alice_balance")        // read
  tr.set("alice_balance", balance - 100)   // write (buffered locally)
  tr.set("bob_balance", bob_bal + 100)     // write (buffered locally)
  tr.commit()                              // send to cluster

NOTHING goes to the cluster during reads/writes.
All reads are from a SNAPSHOT. All writes are buffered locally.
Only at COMMIT does the cluster get involved.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │ Step 1: BEGIN TRANSACTION                                   │
  │   Client gets a READ VERSION from the Sequencer.            │
  │   This is a monotonically increasing timestamp.             │
  │   All reads in this txn will see data AS OF this version.  │
  │   (Snapshot isolation — consistent point-in-time view.)     │
  │                                                              │
  │ Step 2: READS                                               │
  │   Client sends reads to the appropriate Storage Servers.    │
  │   Storage server returns the value at the read version.     │
  │   Client also records: "I read key X" (the READ SET).      │
  │                                                              │
  │ Step 3: WRITES (local only)                                 │
  │   Client buffers writes in memory.                          │
  │   Nothing is sent to the cluster yet.                       │
  │   This is the WRITE SET.                                    │
  │                                                              │
  │ Step 4: COMMIT                                              │
  │   Client sends to a Proxy:                                  │
  │     - Read set: [keys I read]                               │
  │     - Write set: [keys + values I want to write]            │
  │     - Read version: the snapshot version I read at          │
  │                                                              │
  │ Step 5: Proxy gets a COMMIT VERSION from Sequencer          │
  │   Commit version > read version. This serializes all txns.  │
  │                                                              │
  │ Step 6: Proxy sends to RESOLVER(s)                          │
  │   Resolver checks: "Has any key in the READ SET been        │
  │   WRITTEN by another committed transaction between the      │
  │   read version and commit version?"                         │
  │                                                              │
  │   If YES → CONFLICT → abort. Client retries automatically. │
  │   If NO  → no conflict → proceed to commit.                │
  │                                                              │
  │ Step 7: Proxy writes to LOG SERVERS                         │
  │   Log servers durably store the write set.                  │
  │   When majority of log servers confirm → transaction is     │
  │   COMMITTED. Return success to client.                      │
  │                                                              │
  │ Step 8: Log servers ASYNCHRONOUSLY push data to Storage     │
  │   Storage servers apply the writes to their B-trees.        │
  │   This is async — the client already got its commit ack.    │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

### 3.2 Conflict Detection — How OCC Works

```
OCC means: "be optimistic — assume no conflicts. Check at commit time."

  Transaction A:                Transaction B:
  read version = 100            read version = 100
  reads: alice_balance (key X)  reads: alice_balance (key X)
  writes: alice_balance = 50    writes: alice_balance = 70
  commit → version 101          commit → version 102

  Commit order:
    A commits first at version 101.
    B commits next at version 102.

  Resolver checks B:
    "B read key X at version 100. Was key X written between 100 and 102?"
    YES — transaction A wrote key X at version 101.
    → B is ABORTED (conflict).
    → B automatically retries with a fresh read version (102).

  This is SERIALIZABLE isolation — the strongest level.
  The result is the same as if transactions ran one-at-a-time.

  WHY OCC instead of locks:
    - No locks held during the "think" phase (reads + compute)
    - Higher concurrency when conflicts are rare
    - No deadlocks (can't deadlock without locks)
    - Simple recovery (just abort and retry)
    - Downside: high-contention workloads → lots of retries
```

### 3.3 The Five-Second Rule

```
FoundationDB enforces a HARD LIMIT: transactions must complete within 5 seconds.

  If your transaction takes longer than 5 seconds → it's automatically aborted.

  WHY:
    1. The read version becomes stale. The longer you hold it, the more
       likely other txns have written to your read set → conflict → retry
       → wasted work.
    2. The Resolver must remember ALL committed write sets since the oldest
       active read version. Long transactions → huge memory usage in Resolvers.
    3. Forces good design: keep transactions small and fast.

  This means: NO long-running transactions. No "batch update 1M rows in one txn."
  Instead: break into many small transactions. The client library provides
  helpers for this (e.g., get_range with continuation).

  Also: max transaction size = 10 MB of writes.
  This is fine for metadata operations. Not for bulk data loads.
```

---

## 4. The Sequencer — Global Ordering Without Distributed Clocks

```
Spanner uses GPS clocks + atomic clocks (TrueTime) for global ordering.
FoundationDB takes a simpler approach: a SINGLE SEQUENCER.

  The Sequencer is one process that hands out monotonically increasing
  version numbers:

    Transaction A asks for read version  → gets 100
    Transaction B asks for read version  → gets 101
    Transaction A asks for commit version → gets 102
    Transaction C asks for read version  → gets 103

  This gives you a TOTAL ORDER on all transactions. Simple. Correct.

  "But isn't a single sequencer a bottleneck?"

  The Sequencer does almost no work — it just increments a counter
  and returns it. This takes ~1μs. It can hand out millions of versions
  per second. In practice, it's never the bottleneck.

  "What if the Sequencer crashes?"

  FoundationDB does a "recovery": elect a new Sequencer, replay the
  transaction log, resume. Takes ~1-5 seconds. During this window,
  the cluster is unavailable (but no data is lost).

  This is a deliberate tradeoff:
    SIMPLICITY (single sequencer) > AVAILABILITY (brief outage on failure)
    FoundationDB chooses CP in CAP theorem, strongly.
```

---

## 5. Storage Engine — Ordered Keys on Disk

```
Each Storage Server stores a range of the key space.

  Internal storage: SQLite-based B-tree (historically)
  Newer: "Redwood" — a custom copy-on-write B-tree designed for SSD.

  Keys are stored in SORTED ORDER. This means:
    - Point reads: O(log N) B-tree traversal
    - Range scans: sequential read from B-tree leaf pages
    - Prefix queries: everything with prefix "users/" is contiguous

  ┌──────────────────────────────────────────────────────────────┐
  │  Storage Server for range [accounts/a, accounts/m)          │
  │                                                              │
  │  B-tree:                                                     │
  │  ┌──────────────────────────────────────┐                   │
  │  │ accounts/alice   → {balance: 500}    │                   │
  │  │ accounts/bob     → {balance: 1200}   │                   │
  │  │ accounts/carol   → {balance: 50}     │                   │
  │  │ accounts/dave    → {balance: 8000}   │                   │
  │  │ ...                                  │                   │
  │  └──────────────────────────────────────┘                   │
  │                                                              │
  │  MVCC: each key has multiple versions (for snapshot reads). │
  │  Old versions are garbage-collected after they're no longer │
  │  needed by any active transaction.                          │
  └──────────────────────────────────────────────────────────────┘

  Data is replicated to 3 storage servers (configurable).
  Reads go to the CLOSEST replica (by measured latency).
  This is different from etcd where reads go to the leader —
  FoundationDB reads can go to any replica because MVCC +
  read versions guarantee consistency.
```

---

## 6. Simulation Testing — How FoundationDB Achieves Reliability

```
FoundationDB's most famous engineering practice: DETERMINISTIC SIMULATION.

  The entire FoundationDB cluster — networking, disk I/O, clocks,
  process scheduling — runs inside a SINGLE-THREADED SIMULATOR.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  The simulator can:                                         │
  │    - Run 100 nodes in one process                          │
  │    - Inject arbitrary failures:                             │
  │      - Kill any process at any time                        │
  │      - Partition the network (A can't reach B)             │
  │      - Slow down disk I/O                                  │
  │      - Corrupt disk writes                                 │
  │      - Reorder network packets                             │
  │      - Introduce clock skew                                │
  │    - Run millions of test scenarios per day                │
  │    - REPRODUCE any bug with a single seed number           │
  │                                                              │
  │  Every bug that's ever been found in FoundationDB can be   │
  │  reproduced by running: test --seed=<number>               │
  │                                                              │
  │  This is why FoundationDB has fewer bugs than almost any   │
  │  distributed database. The simulator catches bugs that      │
  │  would take months of production traffic to trigger.        │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  How it works:
    All I/O goes through an abstraction layer ("flow").
    In production: flow → real syscalls (read, write, send, recv).
    In testing: flow → simulated I/O with controllable failures.

    Because the simulator is DETERMINISTIC (single-threaded,
    controlled randomness with a seed), any test failure can be
    replayed by re-running with the same seed.

  This testing approach is now influential:
    TigerBeetle, Antithesis (company), and others have adopted
    similar deterministic simulation testing.
```

---

## 7. The Layer Concept — Building Databases on FoundationDB

```
FoundationDB provides ONLY ordered key-value pairs + ACID transactions.
Everything else is built as a "layer" on top.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Layer: Document Store (like MongoDB)                       │
  │    Encode documents as: /docs/{collection}/{id} → JSON      │
  │    Secondary indexes: /idx/{collection}/{field}/{value}/{id} │
  │    Use transactions to keep indexes consistent with docs.    │
  │                                                              │
  │  Layer: SQL Database                                        │
  │    Tables: /tables/{table}/{pk} → row bytes                 │
  │    Indexes: /idx/{table}/{index_name}/{column_val}/{pk}     │
  │    Schema: /schema/{table} → column definitions             │
  │    JOIN = multiple key reads in one transaction.             │
  │                                                              │
  │  Layer: Message Queue                                       │
  │    Queue: /queues/{name}/{sequence_number} → message        │
  │    Consumer: /consumers/{queue}/{consumer_id} → last_offset │
  │    Enqueue + dequeue in one transaction = exactly-once.     │
  │                                                              │
  │  Layer: Object Store (S3-like)                              │
  │    Metadata: /objects/{bucket}/{key} → {size, chunks}       │
  │    Chunks:   /chunks/{chunk_id} → raw bytes                 │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Apple's Record Layer (open-sourced):
    A structured data store with:
      - Schema with typed fields
      - Secondary indexes (maintained transactionally)
      - Query planner
      - Versioned schema evolution
    Powers CloudKit (iCloud backend). Billions of records.

  WHY this design:
    Building ACID transactions is the hardest part of a database.
    FoundationDB solves it ONCE. Layers build data models on top
    without reimplementing concurrency control, replication, recovery.
    Correct by construction.
```

---

## 8. Recovery — What Happens When Things Fail

```
FoundationDB's recovery model is unique: FAST RESTART instead of
COMPLEX STATE MANAGEMENT.

  When any critical process fails (sequencer, resolver, log server):
    1. Coordinators detect the failure
    2. ENTIRE transaction system is shut down
    3. New transaction system processes are recruited
    4. New processes replay uncopied data from old log servers
    5. System resumes accepting transactions

  ┌──────────────────────────────────────────────────────────────┐
  │  Recovery timeline:                                         │
  │                                                              │
  │  t=0s    Process fails                                      │
  │  t=0.5s  Failure detected                                   │
  │  t=1-3s  New transaction system started                     │
  │  t=1-5s  Log replay complete                                │
  │  t=1-5s  Resume accepting transactions                      │
  │                                                              │
  │  Total unavailability: ~1-5 seconds.                        │
  │  NO DATA LOST. Committed transactions are in the log.       │
  │  In-flight (uncommitted) transactions are aborted and       │
  │  automatically retried by the client library.               │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  WHY this works:
    The transaction system is STATELESS (aside from the log).
    Storage servers are unaffected by transaction system recovery.
    You're just replacing a few processes and replaying recent log entries.

    Compare to Raft: leader election takes 1-2 seconds but you only
    replace ONE leader. FoundationDB replaces the ENTIRE transaction
    system but it's also 1-5 seconds because the system is designed
    for fast restarts.
```

---

## 9. FoundationDB vs. the World

```
┌──────────────────┬────────────────┬────────────────┬────────────────┐
│                  │ FoundationDB   │ CockroachDB    │ etcd           │
├──────────────────┼────────────────┼────────────────┼────────────────┤
│ Data model       │ Ordered KV     │ SQL tables     │ Flat KV        │
│ Transactions     │ Serializable   │ Serializable   │ Mini-txn (CAS) │
│                  │ ACID           │ ACID           │                │
│ Intended data    │ Terabytes      │ Terabytes      │ <8 GB          │
│ Protocol         │ Paxos-like     │ Raft per range │ Raft           │
│ Ordering         │ Single         │ Hybrid logical │ Raft leader    │
│                  │ Sequencer      │ clock          │                │
│ Conflict detect  │ OCC (at commit)│ Write intents  │ CAS at leader  │
│ Multi-DC         │ Yes (async or  │ Yes (sync)     │ Not designed   │
│                  │ sync)          │                │ for multi-DC   │
│ Testing          │ Deterministic  │ Jepsen +       │ Jepsen         │
│                  │ simulation     │ roachtest      │                │
│ Best for         │ Infrastructure │ Application    │ Coordination   │
│                  │ building block │ database       │ metadata       │
│ Complexity       │ Must build your│ Ready to use   │ Ready to use   │
│                  │ own data model │ (SQL)          │ (simple KV)    │
└──────────────────┴────────────────┴────────────────┴────────────────┘

FoundationDB is LOWER LEVEL than CockroachDB.
  CockroachDB: use it for your application (SQL, indexes, schema).
  FoundationDB: use it to BUILD a database, queue, or metadata store.
```

---

## 10. Key Numbers

```
Write throughput:         ~500K writes/sec (per cluster, scales with nodes)
Read throughput:          millions/sec (scales linearly with storage servers)
Write latency:            ~2-5ms (commit, dependent on log server fsync)
Read latency:             ~0.5-1ms (from nearest storage server)
Transaction size limit:   10 MB
Transaction time limit:   5 seconds
Key size limit:           10 KB
Value size limit:         100 KB
Cluster size:             hundreds of nodes in production (Apple)
Recovery time:            1-5 seconds
Replication:              3× by default (configurable)
Consistency:              Serializable (always, no weaker option)
```

---

## 11. When to Use FoundationDB

```
USE when:
  - You're building infrastructure that needs ACID transactions at scale
  - Your data model doesn't fit SQL (custom indexing, graph, document, queue)
  - You want to build a database with correct concurrency for free
  - You need serializable isolation and can't tolerate anomalies
  - Apple, Snowflake, and Wavefront already validated it for your scale

DON'T USE when:
  - You just need a SQL database → use CockroachDB or Postgres
  - You need >100 KB values → not designed for blobs
  - You need transactions longer than 5 seconds
  - You want a fully managed service → no major cloud offers managed FDB
  - You're a small team → the "build your own layer" overhead may not be worth it
```

---

## 12. Deep Dive: Simulation Testing Architecture

### 12.1 The Flow Runtime

FDB is written in **Flow**, a custom C++ extension (transpiled to C++) that provides
cooperative multitasking via coroutines — similar to async/await:

```cpp
// Flow code (simplified) — looks like normal async code
ACTOR Future<Void> doWrite(Database db, Key key, Value val) {
    state Transaction tr(db);
    loop {
        try {
            tr.set(key, val);
            wait(tr.commit());  // yields here, resumes later
            return Void();
        } catch (Error& e) {
            wait(tr.onError(e));  // retry logic
        }
    }
}
```

The `ACTOR` macro and `wait()` are the key. Every `wait()` is a yield point — the coroutine
suspends and the scheduler picks the next one to run.

### 12.2 Why Single-Threaded Makes It Deterministic

```
Real production:                    Simulator:
┌─────────────────────┐            ┌─────────────────────┐
│ Thread 1: Node A    │            │                     │
│ Thread 2: Node B    │            │  Single thread:     │
│ Thread 3: Node C    │            │                     │
│ OS schedules threads│            │  Event queue:       │
│ (non-deterministic) │            │   [A:recv, B:disk,  │
│                     │            │    C:timer, A:send]  │
│ Real network (async)│            │                     │
│ Real disk (async)   │            │  Seed: 12345        │
│ Real clock           │            │  → always same order│
└─────────────────────┘            └─────────────────────┘
```

In production, OS thread scheduling is non-deterministic. Two runs of the same test produce
different interleavings. In the simulator:

1. **All nodes run as coroutines in one thread.** No OS scheduling randomness.
2. **A priority queue of events** determines what happens next, ordered by simulated time.
3. **A seeded PRNG** controls all "random" decisions — which coroutine runs, how long disk
   I/O takes, when failures are injected. Same seed → same execution → same bug → reproducible.

### 12.3 The Abstraction Layer (INetwork, IDisk, IAsyncFile)

The entire codebase never calls real syscalls directly. Everything goes through interfaces:

```
Production path:                    Simulation path:

Code calls:                         Code calls:
  INetwork::send(packet)              INetwork::send(packet)
       │                                   │
       ▼                                   ▼
  Net2 (real TCP sockets)            Sim2 (in-memory queues)
  - actual send()                    - put packet in event queue
  - actual recv()                    - deliver after simulated delay
  - real latency                     - maybe drop/reorder/delay it

Code calls:                         Code calls:
  IAsyncFile::write(data)             IAsyncFile::write(data)
       │                                   │
       ▼                                   ▼
  AsyncFileNonDurable (real IO)      SimDisk
  - actual pwrite()                  - in-memory buffer
  - actual fsync()                   - maybe corrupt it
                                     - maybe lose it (simulated crash)

Code calls:                         Code calls:
  now()                               now()
       │                                   │
       ▼                                   ▼
  Real wall clock                    Simulated clock
                                     - can jump, skew, pause
```

The production code and test code are **identical** — only the runtime implementation differs.

### 12.4 BUGGIFY: Injecting Chaos From Inside

The FDB codebase is peppered with `BUGGIFY` macros:

```cpp
// In actual FDB source code (simplified)
Future<Void> storageServer(/* ... */) {
    // ...
    if (BUGGIFY) {
        // 1% of sim runs: artificially slow down this operation
        wait(delay(deterministicRandom()->random01() * 5.0));
    }

    state int bytesWritten = write(data);

    if (BUGGIFY) {
        // Simulate a partial write (disk corruption)
        bytesWritten = deterministicRandom()->randomInt(0, bytesWritten);
    }
    // ...
}
```

- In **production**: `BUGGIFY` always evaluates to `false`. Zero overhead.
- In **simulation**: `BUGGIFY` is true some percentage of the time (controlled by seed).

There are **thousands** of BUGGIFY points scattered throughout the codebase. They simulate:
  - Slow disk writes
  - Partial writes (torn pages)
  - Process crashes at specific points
  - Memory pressure
  - Network connection resets
  - Delayed responses

This means the code tests its own fault-tolerance paths — the rare edge cases that would
take months of production traffic to hit.

### 12.5 The Simulation Event Loop

```
simulator_main(seed=12345):
    rng = PRNG(seed)
    event_queue = PriorityQueue()  // ordered by simulated_time

    // Create simulated cluster
    for i in 0..N:
        spawn_simulated_process(event_queue, rng)  // each node = a coroutine

    // Run
    while event_queue.not_empty():
        event = event_queue.pop_earliest()
        simulated_time = event.time

        // Maybe inject a fault right now
        if rng.random() < fault_probability:
            inject_fault(rng)  // kill process, partition network, etc.

        // Execute one step of one coroutine
        event.coroutine.resume()
        // Coroutine runs until next wait(), then yields
        // Any new events (sends, timers) go into event_queue
```

This is a **discrete event simulator**. Time doesn't flow — it jumps from event to event.
A 10-minute simulated test might complete in 2 seconds of wall time because there's no
actual waiting.

### 12.6 What the Test Harness Checks

After running a simulated workload with injected failures:

```
1. No data loss:     committed transactions are never lost
2. Serializability:  no transaction sees impossible state
3. Availability:     cluster recovered after failures
4. Consistency:      all replicas converge after partition heals
```

If any invariant breaks → the seed is logged → developer re-runs with that seed and uses
a debugger to step through the exact sequence. Single-threaded = single-steppable in gdb.

### 12.7 What FDB Gave Up for Simulation

```
1. Custom language (Flow)     — can't use standard C++ async or threads
2. No third-party libraries   — everything must go through the abstraction
                                (can't use a random HTTP library that does real IO)
3. Single-threaded core       — limits per-core throughput
                                (compensated by running one process per core)
4. Massive discipline         — every engineer must use INetwork/IDisk/etc.
                                One real syscall in the wrong place breaks the
                                entire simulation guarantee
```

### 12.8 The Payoff

```
FDB runs ~30 million simulation tests per day in CI.
Each with a random seed, random fault injection.

Result: bugs that would take years of production traffic to find
        get caught in hours.

The famous stat: FDB had ZERO known data loss bugs in production
when Apple used it as the backing store for CloudKit (iCloud).
```

### 12.9 Summary Table

```
┌────────────────────┬──────────────────────────────────────────────────────┐
│ Concept            │ What it does                                         │
├────────────────────┼──────────────────────────────────────────────────────┤
│ Flow               │ Custom coroutine runtime — cooperative multitasking, │
│                    │ all code is async actors                             │
├────────────────────┼──────────────────────────────────────────────────────┤
│ Single thread      │ One thread runs all simulated nodes → deterministic  │
│                    │ ordering → reproducible                              │
├────────────────────┼──────────────────────────────────────────────────────┤
│ INetwork/IDisk     │ Abstraction layer — production uses real IO,         │
│                    │ simulator uses fakes                                 │
├────────────────────┼──────────────────────────────────────────────────────┤
│ Seed               │ One number controls all randomness → same seed =    │
│                    │ same execution = same bug                            │
├────────────────────┼──────────────────────────────────────────────────────┤
│ BUGGIFY            │ Thousands of fault-injection points inside the code, │
│                    │ active only in simulation                            │
├────────────────────┼──────────────────────────────────────────────────────┤
│ Event queue        │ Discrete event simulation — time jumps, no real      │
│                    │ waiting, runs millions of tests fast                 │
└────────────────────┴──────────────────────────────────────────────────────┘
```
