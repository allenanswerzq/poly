# Distributed Key-Value Stores — How They're Designed

How systems like DynamoDB, etcd, FoundationDB, Cassandra, and Riak
handle partitioning, replication, consistency, and storage at scale.

---

## 1. Why Distributed? Single-Node Isn't Enough

```
A single Redis or RocksDB can handle:
  - Reads:    ~500K ops/sec
  - Storage:  limited to one machine's disk (~10 TB practical)
  - Memory:   limited to one machine's RAM (~256 GB)

When your data exceeds one machine, or you need fault tolerance
(one machine dying = total outage), you need DISTRIBUTION.

A distributed KV store solves three problems:
  1. Scale beyond one machine (partition data across nodes)
  2. Survive failures (replicate data so no single point of failure)
  3. Stay fast (route requests to the nearest replica)

The hard part: these goals CONFLICT with each other.
  More replicas = more durability but harder to keep consistent.
  More partitions = more throughput but harder to do range queries.
  Strong consistency = safer but slower.

Every distributed KV store makes different tradeoffs.
Understanding the design space lets you pick the right one.
```

---

## 2. The Design Space — Decisions Every System Must Make

```
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│    Decision 1: Partitioning                                         │
│    How do you split data across machines?                           │
│                                                                      │
│    Decision 2: Replication                                          │
│    How many copies? How do you keep them in sync?                   │
│                                                                      │
│    Decision 3: Consistency model                                    │
│    What guarantees do clients see?                                  │
│                                                                      │
│    Decision 4: Storage engine (per-node)                            │
│    B-tree or LSM-tree? What's on disk?                             │
│                                                                      │
│    Decision 5: Failure handling                                     │
│    What happens when a node dies? Network partitions?               │
│                                                                      │
│    Decision 6: Conflict resolution                                  │
│    Two writes to the same key at the same time?                     │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. Partitioning — Splitting Data Across Nodes

### 3.1 Hash Partitioning

```
Partition = hash(key) % N

  key="alice" → hash = 7492 → 7492 % 3 = 1 → Node 1
  key="bob"   → hash = 3841 → 3841 % 3 = 2 → Node 2
  key="carol" → hash = 1200 → 1200 % 3 = 0 → Node 0

  Pros:
    - Even distribution (good hash = uniform spread)
    - O(1) routing (just hash and mod)

  Cons:
    - Adding/removing a node: hash(key) % N changes for MOST keys
      → massive data movement (rehashing problem)
    - Range queries impossible ("all keys between alice and diana")
      because adjacent keys hash to random nodes
```

### 3.2 Consistent Hashing (DynamoDB, Cassandra, Riak)

```
Instead of modding by N, put nodes on a hash RING:

         Node A (pos 0)
          ╱          ╲
    Node D              Node B
    (pos 270)          (pos 90)
          ╲          ╱
         Node C (pos 180)

  To assign a key: hash(key) → position on ring → walk clockwise
  until you hit a node. That node owns the key.

  hash("alice") = 45   → between A(0) and B(90)   → Node B owns it
  hash("bob")   = 200  → between C(180) and D(270) → Node D owns it

  ADD a new node E at position 135:
    Only keys between 90-135 need to move (from C to E).
    All other keys stay. Minimal data movement.

  REMOVE Node C:
    Only keys C owned move to the next node (D).

  Virtual nodes: each physical node gets ~100-200 positions on the ring.
    This fixes the "uneven distribution" problem when you only have 3 nodes.
    DynamoDB and Cassandra both use virtual nodes.

  ┌──────────────────────────────────────────────────────────────┐
  │  Without virtual nodes:     With virtual nodes:              │
  │                                                              │
  │  Node A: 33% of ring       Node A (50 positions): ~33%     │
  │  Node B: 15% of ring       Node B (50 positions): ~33%     │
  │  Node C: 52% of ring       Node C (50 positions): ~33%     │
  │  (uneven!)                  (much more even)                │
  └──────────────────────────────────────────────────────────────┘
```

### 3.3 Range Partitioning (Bigtable, HBase, CockroachDB, TiKV)

```
Split the key space into contiguous ranges:

  Node 0: keys [     , "g")     → "alice", "bob", "carol"
  Node 1: keys ["g"  , "p")     → "george", "Karen", "mike"
  Node 2: keys ["p"  , ∞)       → "peter", "zach"

  Pros:
    - Range queries are efficient ("all keys a* to d*" → hits one node)
    - Sorted within each partition → good for scans, time-series

  Cons:
    - Hot spots: if all keys start with "user_" → one node gets everything
    - Must handle split/merge as data grows:
      Node 0 gets too big → split into [,"d") and ["d","g")
    - More complex than hash partitioning

  CockroachDB/TiKV approach:
    Ranges auto-split at ~128 MB. Leader of each range is elected
    via Raft. The range can move between nodes for load balancing.
```

---

## 4. Replication — Keeping Copies in Sync

### 4.1 Leader-Based Replication (etcd, CockroachDB, TiKV, DynamoDB)

```
ONE node is the leader for each partition. All writes go through it.
Leader replicates to followers.

  Write: Client → Leader → {Follower 1, Follower 2} → Ack

  ┌──────────┐      ┌──────────┐      ┌──────────┐
  │  Leader   │─────→│Follower 1│      │Follower 2│
  │  (writes) │─────→│ (reads)  │      │ (reads)  │
  └──────────┘      └──────────┘      └──────────┘
       │                 │                  │
    All writes        Can serve           Can serve
    go here           stale reads         stale reads

  Strong consistency option:
    Write waits for MAJORITY (2 of 3) to confirm before acking.
    This is what Raft and Paxos provide.

  Leader dies:
    Followers detect missing heartbeat → elect new leader.
    Brief unavailability during election (typically < 5 seconds).
```

### 4.2 Leaderless Replication (Cassandra, Riak, Original Dynamo)

```
NO leader. Client writes to MULTIPLE nodes directly.
Uses quorum to determine success.

  Replication factor R=3, Write quorum W=2, Read quorum Q=2

  Write "alice=100":
    Client → Node 1 ✓
    Client → Node 2 ✓    ← 2 of 3 confirmed, return success
    Client → Node 3 ✗    ← slow/down, doesn't matter

  Read "alice":
    Client → Node 1: "alice=100"
    Client → Node 3: "alice=50" (stale!)    ← only 2 of 3 needed
    Client picks highest version → "alice=100"

  Quorum formula:
    W + R > N  guarantees overlap between write and read sets.
    At least one node in the read set saw the latest write.

    N=3, W=2, R=2:  2+2=4 > 3 ✓  → strong consistency
    N=3, W=1, R=1:  1+1=2 ≤ 3 ✗  → eventual consistency (but faster)
    N=3, W=3, R=1:  3+1=4 > 3 ✓  → strong (slow writes, fast reads)

  ┌──────────────────────────────────────────────────────────────┐
  │  Leader-based:    Simple, strong consistency, single point   │
  │                   of bottleneck for writes per partition.     │
  │                                                              │
  │  Leaderless:      No single point of failure, always writable│
  │                   but harder to get strong consistency.       │
  │                   Must handle conflicts (next section).       │
  └──────────────────────────────────────────────────────────────┘
```

### 4.3 Anti-Entropy — Repairing Stale Replicas

```
In leaderless systems, replicas drift apart. Two repair mechanisms:

  1. Read Repair
     During a quorum read, if one replica returns stale data,
     the coordinator sends the latest version to the stale replica.
     Free — happens on every read. But only fixes what's read.

  2. Merkle Tree Anti-Entropy
     Background process compares Merkle trees (hash trees) of
     each replica's data. Differences → sync only the differing keys.

     ┌─────────────────────────────────────────────────────────┐
     │  Merkle trees:                                          │
     │                                                         │
     │  Node A:           Node B:                              │
     │     [H_root]          [H_root']  ← roots differ!       │
     │    /        \        /        \                         │
     │  [H_AB]   [H_CD]  [H_AB']  [H_CD]  ← left differs     │
     │  / \      / \      / \      / \                         │
     │ A   B    C   D    A'  B    C   D    ← A is different    │
     │                                                         │
     │  Only need to transfer key A. Not the entire dataset.   │
     │  Cassandra, Riak, DynamoDB all use Merkle trees.        │
     └─────────────────────────────────────────────────────────┘
```

---

## 5. Consistency Models — The Spectrum

```
From weakest to strongest:

  ┌─────────────────────────────────────────────────────────────────┐
  │                                                                 │
  │  Eventual Consistency                                          │
  │    "If you stop writing, all replicas EVENTUALLY converge."    │
  │    No guarantees about WHEN. You might read stale data.        │
  │    Used by: Cassandra (default), S3 (pre-2020), DNS            │
  │                                                                 │
  │  Read-Your-Writes                                              │
  │    "You always see your own writes."                           │
  │    Others might see stale data. Session consistency.            │
  │    Used by: DynamoDB (with consistent read), most web apps     │
  │                                                                 │
  │  Causal Consistency                                            │
  │    "If A caused B, everyone sees A before B."                  │
  │    Concurrent operations can be seen in any order.             │
  │    Used by: MongoDB (with causal sessions)                     │
  │                                                                 │
  │  Linearizability (strongest)                                   │
  │    "The system behaves as if there's ONE copy of data."        │
  │    Every read returns the most recent write. Period.           │
  │    Used by: etcd, ZooKeeper, CockroachDB, Spanner             │
  │                                                                 │
  │  Strength: Eventual < Read-Your-Writes < Causal < Linearizable│
  │  Latency:  Eventual is fastest. Linearizable is slowest.      │
  │                                                                 │
  └─────────────────────────────────────────────────────────────────┘

WHY does stronger consistency cost more latency?

  Linearizable write:
    Client → Leader → {wait for majority of replicas to confirm} → Ack
    Must wait for network round trips to replicas.

  Eventually consistent write:
    Client → Any node → Ack immediately.
    Replication happens asynchronously in background.

  The difference is ~1-5ms vs ~0.1ms within a datacenter.
  Cross-datacenter: ~50-200ms vs ~1ms. That's where it REALLY hurts.
```

---

## 6. Storage Engines — What's on Each Node

Each node in a distributed KV store runs a local storage engine. The two
dominant designs:

### 6.1 B-Trees (DynamoDB, etcd, CockroachDB on Pebble)

```
B-tree: sorted key-value pairs stored in fixed-size pages on disk.

  ┌─────────────────────────────────────────────────────────────┐
  │                     Root Page                                │
  │            [key10  |  key20  |  key30]                      │
  │           ╱         │         │        ╲                     │
  │    [<10]       [10-20]    [20-30]     [>30]                │
  │      │            │          │           │                   │
  │    Page 1       Page 2     Page 3      Page 4               │
  │   [k1:v1]     [k11:v11]  [k21:v21]  [k31:v31]             │
  │   [k2:v2]     [k12:v12]  [k22:v22]  [k32:v32]             │
  │   [k5:v5]     [k15:v15]  [k25:v25]  [k38:v38]             │
  └─────────────────────────────────────────────────────────────┘

  READ: traverse from root → O(log N) page reads. Typically 3-4 levels
  for billions of keys (each page holds ~100-500 entries).

  WRITE: find the leaf page, update in place. WAL first for crash safety.
  If page is full → split into two pages, update parent pointer.

  Pros: Fast reads (especially point lookups), well-understood.
  Cons: Random I/O for writes (updating pages in place), write amplification.
```

### 6.2 LSM-Trees (Cassandra, RocksDB, LevelDB, ScyllaDB)

```
LSM-tree: write to memory first, flush sorted runs to disk, merge in background.

  ┌─────────────────────────────────────────────────────────────┐
  │                                                             │
  │  MEMORY:                                                    │
  │  ┌──────────────────────────┐                              │
  │  │  MemTable (sorted)       │  ← all writes go here first │
  │  │  key5:v5, key8:v8, ...   │    (red-black tree or skiplist)
  │  └──────────┬───────────────┘                              │
  │             │ flush when full (~64 MB)                      │
  │             ▼                                               │
  │  DISK:                                                      │
  │  ┌──────────────────────────┐                              │
  │  │  Level 0: SSTable files  │  Recently flushed, may overlap│
  │  └──────────────────────────┘                              │
  │  ┌──────────────────────────┐                              │
  │  │  Level 1: SSTable files  │  Non-overlapping, sorted      │
  │  └──────────────────────────┘  Each level ~10× bigger        │
  │  ┌──────────────────────────┐                              │
  │  │  Level 2: SSTable files  │                              │
  │  └──────────────────────────┘                              │
  │                                                             │
  │  READ: MemTable → L0 → L1 → L2 → ... (check each level)  │
  │  Bloom filters skip most levels (false positive ~1%).       │
  │                                                             │
  │  COMPACTION: background merge of SSTables at each level.   │
  │  Removes deleted keys, resolves duplicates, keeps sorted.  │
  │                                                             │
  └─────────────────────────────────────────────────────────────┘

  Pros: Sequential writes only (fast, SSD/HDD friendly),
        high write throughput, space-efficient compaction.
  Cons: Read amplification (may check multiple levels),
        compaction uses CPU and I/O.
```

### 6.3 B-Tree vs LSM-Tree

```
┌──────────────────┬──────────────────┬──────────────────────────┐
│                  │ B-Tree           │ LSM-Tree                 │
├──────────────────┼──────────────────┼──────────────────────────┤
│ Write pattern    │ Random I/O       │ Sequential I/O           │
│ Write speed      │ Moderate         │ Very fast                │
│ Read speed       │ Fast (1 seek)    │ Moderate (multi-level)   │
│ Space usage      │ Higher (pages    │ Lower (compacted)        │
│                  │ half-full avg)   │                          │
│ Write amplif.    │ ~10-30×          │ ~10-30× (compaction)     │
│ Predictable      │ Yes              │ Compaction spikes        │
│ Used by          │ Postgres, MySQL  │ Cassandra, RocksDB,      │
│                  │ DynamoDB, etcd   │ LevelDB, ScyllaDB        │
├──────────────────┼──────────────────┼──────────────────────────┤
│ Best for         │ Read-heavy,      │ Write-heavy,             │
│                  │ point lookups    │ high ingestion rate       │
└──────────────────┴──────────────────┴──────────────────────────┘
```

---

## 7. Conflict Resolution — Two Writes to the Same Key

```
In a distributed system, two clients can write to the same key
on different replicas at the same time (especially in leaderless systems).

  Client A writes key="balance", value=100 → Node 1
  Client B writes key="balance", value=200 → Node 2
  (simultaneously, during a network partition)

  Which value wins? You have three options:

  1. Last-Writer-Wins (LWW) — simplest, most common
     Each write carries a timestamp. Highest timestamp wins.
     Used by: Cassandra, DynamoDB

     Problem: timestamps can be wrong. Clock skew = data loss.
     Client A's write at t=100 is silently overwritten by
     Client B's write at t=101, even if A's write was "later"
     in real time.

     LWW = "I accept that concurrent writes may be lost."

  2. Vector Clocks — detect conflicts, let app resolve
     Each node maintains a vector of (node, counter) pairs.
     If neither vector dominates the other → CONFLICT.
     Return both versions to the client, let them merge.
     Used by: Riak, original Dynamo paper

     ┌─────────────────────────────────────────────────────┐
     │  Event sequence:                                     │
     │  Client writes v1:  clock = {A:1}                   │
     │  Client A writes v2: clock = {A:2}                  │
     │  Client B writes v3: clock = {A:1, B:1}             │
     │                                                      │
     │  {A:2} and {A:1, B:1} are CONCURRENT (neither       │
     │  dominates). Read returns BOTH. Client must merge.   │
     └─────────────────────────────────────────────────────┘

  3. Consensus (prevent conflicts in the first place)
     Use Raft/Paxos so only ONE leader handles writes per key.
     Concurrent writes are serialized by the leader.
     Used by: etcd, CockroachDB, TiKV, FoundationDB

     No conflicts, but writes are slower (must reach consensus).
```

---

## 8. How Real Systems Make These Choices

```
┌─────────────┬───────────────┬──────────────┬──────────────┬──────────────┐
│             │ DynamoDB      │ Cassandra    │ etcd         │ FoundationDB │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Partition   │ Hash (auto-   │ Consistent   │ No partition │ Range-based  │
│             │ split ranges) │ hashing +    │ (small data  │ (auto-split) │
│             │               │ vnodes       │ <~8 GB)      │              │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Replication │ Leader-based  │ Leaderless   │ Leader-based │ Leader-based │
│             │ (Paxos, 3 AZ)│ (quorum)     │ (Raft)       │ (Paxos-like) │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Consistency │ Eventual OR   │ Tunable      │ Linearizable │ Serializable │
│             │ Strong (per   │ (W+R>N for   │ (always)     │ (ACID txns)  │
│             │ request)      │ strong)      │              │              │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Storage     │ B-tree        │ LSM-tree     │ B-tree       │ LSM-tree     │
│ engine      │ (custom)      │ (custom)     │ (bbolt)      │ (custom)     │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Conflicts   │ LWW           │ LWW          │ Consensus    │ Consensus    │
│             │               │              │ (no conflict)│ (OCC/MVCC)   │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Transactions│ Single-item   │ Lightweight  │ Mini-txn     │ Full ACID    │
│             │ (or txn table)│ transactions │ (compare-    │ multi-key    │
│             │               │ (Paxos)      │ and-swap)    │ transactions │
├─────────────┼───────────────┼──────────────┼──────────────┼──────────────┤
│ Best for    │ Web apps,     │ Write-heavy, │ Config,      │ Metadata,    │
│             │ serverless    │ multi-DC     │ service      │ control      │
│             │               │              │ discovery    │ planes       │
└─────────────┴───────────────┴──────────────┴──────────────┴──────────────┘
```

---

## 9. Consensus Algorithms — How Leaders Are Elected and Writes Are Committed

### 9.1 Raft (etcd, CockroachDB, TiKV, Consul)

```
Raft is the most commonly used consensus algorithm. It guarantees
that all replicas agree on the SAME sequence of writes.

Three roles: Leader, Follower, Candidate.

  NORMAL OPERATION:
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Client: "write X=5"                                        │
  │       │                                                      │
  │       ▼                                                      │
  │  Leader (Node A)                                            │
  │    1. Append to local log: [index=7, term=3, X=5]          │
  │    2. Send AppendEntries RPC to followers                   │
  │       │              │                                       │
  │       ▼              ▼                                       │
  │  Follower B      Follower C                                 │
  │    append ✓        append ✓                                 │
  │    ack →           ack →                                    │
  │       │              │                                       │
  │       └──────┬───────┘                                       │
  │              ▼                                               │
  │  Leader has 3/3 confirmations (majority = 2 of 3 needed)   │
  │    → Mark entry as COMMITTED                                │
  │    → Apply X=5 to state machine                             │
  │    → Return success to client                               │
  │    → Notify followers to apply                              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  LEADER ELECTION:
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Leader dies. Followers stop receiving heartbeats.          │
  │                                                              │
  │  After election timeout (150-300ms, randomized):            │
  │    Follower B becomes Candidate                             │
  │    Increments term to 4                                     │
  │    Votes for itself                                          │
  │    Sends RequestVote RPC to others                          │
  │                                                              │
  │  Follower C receives vote request:                          │
  │    "Is B's log at least as up-to-date as mine? Yes."       │
  │    Grants vote.                                              │
  │                                                              │
  │  B has 2 of 3 votes → becomes Leader for term 4.           │
  │  Starts sending heartbeats.                                 │
  │                                                              │
  │  Randomized timeouts prevent split votes (two candidates    │
  │  simultaneously). If split vote → increment term, retry.    │
  │                                                              │
  │  Election typically completes in <1 second.                 │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

### 9.2 When to Use Consensus vs. Quorum

```
Consensus (Raft/Paxos):
  - All writes go through single leader → serialized
  - Strong consistency guaranteed
  - Limited write throughput (single leader bottleneck)
  - Best for: metadata, config, coordination (small, critical data)
  - Examples: etcd, ZooKeeper, CockroachDB, TiKV

Quorum (Dynamo-style):
  - Writes go to any replica → no single bottleneck
  - Tunable consistency (W+R>N for strong, W=1 for fast)
  - Higher write throughput (distributed across replicas)
  - Best for: large-scale data, multi-DC, high write volume
  - Examples: Cassandra, Riak, DynamoDB
```

---

## 10. Multi-Datacenter Replication

```
The hardest problem: keeping data in sync across datacenters
with 50-200ms network latency.

  Strategy 1: Single-leader, async replication
    One DC has the leader. Other DCs have async followers.
    Writes go to leader DC. Reads can hit local DC (eventually consistent).

    DC-1 (leader)           DC-2 (follower)
    ┌──────────┐    async   ┌──────────┐
    │ Leader   │ ─────────→ │ Follower │
    │ (writes) │   50-200ms │ (reads)  │
    └──────────┘            └──────────┘

    Used by: standard Postgres/MySQL replication
    Problem: 50-200ms write latency for users near DC-2
             (their writes must go to DC-1)

  Strategy 2: Multi-leader (active-active)
    Each DC has a leader. They asynchronously replicate changes.
    Writes go to LOCAL DC → fast. But conflicts are possible.

    DC-1 (leader)           DC-2 (leader)
    ┌──────────┐    async   ┌──────────┐
    │ Leader   │ ◄────────→ │ Leader   │
    │ (writes) │   bidirect │ (writes) │
    └──────────┘            └──────────┘

    Used by: Cassandra, DynamoDB Global Tables, CockroachDB
    Problem: concurrent writes to same key in different DCs → CONFLICT
    Resolution: LWW, vector clocks, or CRDTs

  Strategy 3: Synchronized multi-DC (strong consistency)
    Every write waits for confirmation from MAJORITY of DCs.
    Latency = cross-DC round trip on every write.

    Used by: Google Spanner (TrueTime + GPS clocks)
    Problem: 100-200ms write latency (but globally consistent)
```

---

## 11. Transactions in Distributed KV Stores

```
Single-key operations are easy: one partition handles it atomically.

Multi-key transactions (update keys on DIFFERENT nodes atomically)
are the hard part:

  TWO-PHASE COMMIT (2PC):
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Transaction: transfer $100 from Alice (Node A) to Bob (B)  │
  │                                                              │
  │  Phase 1 (PREPARE):                                         │
  │    Coordinator → Node A: "prepare to deduct $100"           │
  │    Coordinator → Node B: "prepare to add $100"              │
  │    Both lock the rows and reply "ready"                     │
  │                                                              │
  │  Phase 2 (COMMIT):                                          │
  │    If both ready:                                            │
  │      Coordinator → Node A: "commit"                         │
  │      Coordinator → Node B: "commit"                         │
  │      Both apply changes and release locks.                   │
  │    If any NOT ready:                                         │
  │      Coordinator → all: "abort"                             │
  │                                                              │
  │  PROBLEM: If coordinator crashes between Phase 1 and 2,     │
  │  participants are stuck holding locks forever (blocking).    │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Solutions to the blocking problem:
    - 3PC (non-blocking but impractical)
    - Coordinator WAL (log the decision, recover after crash)
    - Raft-replicate the coordinator (what CockroachDB does)

  FoundationDB: uses optimistic concurrency control (OCC).
    Read keys → compute writes → commit atomically.
    If any read key changed since you read it → abort + retry.
    No locks during the "compute" phase → high concurrency.
```

---

## 12. Practical Decision Guide

```
I need to store...             → Use...

  Config / service discovery     etcd or ZooKeeper
  (small, strongly consistent)   (Raft consensus, linearizable)

  Session / cache data           Redis (fast) or DynamoDB (managed)
  (simple KV, high throughput)

  User data for a web app        DynamoDB (managed, auto-scaling)
  (millions of users)            or CockroachDB (need SQL + ACID)

  IoT / time-series sensor data  Cassandra or ScyllaDB
  (massive write volume)         (LSM-tree, multi-DC, eventual OK)

  Financial transactions         CockroachDB, Spanner, or FoundationDB
  (ACID required across keys)    (consensus-based, serializable)

  Multi-region active-active     Cassandra or DynamoDB Global Tables
  (writes in every DC)           (LWW conflict resolution)

  Metadata for infrastructure    FoundationDB (Apple, Snowflake use it)
  (strong consistency + scale)   (serializable ACID, 500K writes/sec)
```

---

## 13. Key Numbers

```
etcd:
  Max data size:     ~8 GB (not for large data)
  Write latency:     ~2-5ms (Raft consensus)
  Read latency:      ~0.1-1ms (local follower read)

DynamoDB:
  Write latency:     <10ms (p99, single-digit ms median)
  Read latency:      <5ms (eventually consistent)
  Max item size:     400 KB
  Throughput:        effectively unlimited (managed)

Cassandra:
  Write latency:     ~1-2ms (local quorum)
  Read latency:      ~2-5ms (local quorum)
  Write throughput:  ~100K-500K writes/sec per node
  Max partition:     ~100 MB (practical)

FoundationDB:
  Write throughput:  ~500K writes/sec per cluster
  Read throughput:   millions/sec
  Transaction limit: 5 second window, 10 MB per txn
  Latency:          ~2-5ms (commit)
```
