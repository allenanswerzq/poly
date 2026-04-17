# Distributed Protocols — From Theory to Rust

## Why This Matters

Distributed systems fail in ways a single machine can't: messages get lost, nodes crash mid-operation, clocks drift. These 10 protocols are the fundamental building blocks for handling those failures. Every database, message queue, and consensus system you use is built on some combination of these.

This project implements each protocol in Rust with working demos. Run `cargo run` to see all 10 in action.

## The Protocols

```
┌──────────────────────────────────────────────────────────────────────────┐
│                                                                          │
│  CONSENSUS (agree on a value)          TRANSACTIONS (all-or-nothing)     │
│  ┌──────────┐  ┌──────────┐           ┌──────────┐  ┌──────────┐       │
│  │  Raft    │  │  Paxos   │           │   2PC    │  │  Saga    │       │
│  │ leader + │  │ prepare/ │           │ vote +   │  │ forward  │       │
│  │ log repl │  │ accept   │           │ commit   │  │ + compen │       │
│  └──────────┘  └──────────┘           └──────────┘  └──────────┘       │
│                                                                          │
│  TIME & ORDERING                       REPLICATION                       │
│  ┌──────────┐  ┌──────────┐           ┌──────────┐  ┌──────────┐       │
│  │ Lamport  │  │ Vector   │           │  Chain   │  │ Gossip   │       │
│  │ Clocks   │  │ Clocks   │           │  Replic. │  │ Protocol │       │
│  └──────────┘  └──────────┘           └──────────┘  └──────────┘       │
│                                                                          │
│  CONFLICT RESOLUTION                   DISTRIBUTED LOCKING               │
│  ┌──────────┐                         ┌──────────┐                      │
│  │  CRDTs   │                         │  Leases  │                      │
│  │ merge w/o│                         │+ Fencing │                      │
│  │ coordin. │                         │  Tokens  │                      │
│  └──────────┘                         └──────────┘                      │
└──────────────────────────────────────────────────────────────────────────┘
```

## 0. State Machine Replication (SMR) — The Core Idea

SMR is not a protocol — it's the **foundational concept** that Raft, Paxos, and Chain Replication all implement.

```
Idea: if N identical state machines start in the same state,
      and apply the SAME commands in the SAME order,
      they all reach the SAME final state.

  Client sends: SET x=1, SET y=2, SET x=3

  Replica A              Replica B              Replica C
  ┌───────────┐          ┌───────────┐          ┌───────────┐
  │ Log:      │          │ Log:      │          │ Log:      │
  │ 1: SET x=1│          │ 1: SET x=1│          │ 1: SET x=1│
  │ 2: SET y=2│          │ 2: SET y=2│          │ 2: SET y=2│
  │ 3: SET x=3│          │ 3: SET x=3│          │ 3: SET x=3│
  ├───────────┤          ├───────────┤          ├───────────┤
  │ State:    │          │ State:    │          │ State:    │
  │ x=3, y=2  │          │ x=3, y=2  │          │ x=3, y=2  │
  └───────────┘          └───────────┘          └───────────┘
  Same log → Same state. Always.

The TWO hard problems:
  1. AGREEMENT: all replicas must agree on the SAME log order
     → Raft, Paxos, Zab, Chain Replication solve this

  2. DETERMINISM: same input → same output, always
     → No randomness, no wall-clock time, no local state in the SM

┌──────────────────────────────────────────────────────────────┐
│ SMR = Consensus (agree on log) + Deterministic State Machine │
│                                                               │
│  Consensus layer:     Raft, Paxos, Zab, Chain Replication    │
│  State machine layer: KV store, database, filesystem          │
│  They're separable — swap one without changing the other.     │
└──────────────────────────────────────────────────────────────┘
```

**Why the LOG is the key insight:**

```
Without log (replicate state directly):
  Replica A: {x:3, y:2}    Replica B: {x:1, y:2}
  Who's right? No way to tell.

With log (replicate commands, derive state):
  Replica A: log = [SET x=1, SET y=2, SET x=3] → {x:3, y:2}
  Replica B: log = [SET x=1, SET y=2]           → {x:1, y:2}
  B is behind! Send entry 3, it catches up. Simple.

Log gives you:
  ✓ Total ordering of operations
  ✓ Easy catch-up (replay from log)
  ✓ Consistency verification (compare log prefixes)
  ✓ Snapshotting (save state, truncate old log entries)
```

**Real-world SMR implementations:**

```
┌─────────────────┬──────────────────┬──────────────────────────────┐
│ System          │ Consensus        │ State Machine                │
├─────────────────┼──────────────────┼──────────────────────────────┤
│ etcd            │ Raft             │ Key-value store (bbolt)      │
│ ZooKeeper       │ Zab              │ Hierarchical KV (znodes)     │
│ CockroachDB     │ Raft (per-range) │ SQL database engine          │
│ TiKV            │ Raft (per-region)│ Key-value store (RocksDB)    │
│ Google Spanner  │ Multi-Paxos      │ SQL database engine          │
│ Consul          │ Raft             │ Service catalog + KV         │
└─────────────────┴──────────────────┴──────────────────────────────┘

Same consensus, different state machines — that's the beauty of SMR.
Your raft.rs demonstrates the full pattern:
  log replication (consensus) + apply_committed() → HashMap (state machine)
```

## 1. Raft — Leader-Based Consensus

**File:** `src/raft.rs`

**Problem:** N nodes need to agree on an ordered log of commands, even if some nodes crash.

```
How it works:
  1. LEADER ELECTION
     - Nodes start as Followers, timeout → become Candidate
     - Candidate requests votes from all peers
     - Majority votes → become Leader (only 1 leader per term)
     - Term numbers prevent stale leaders

  2. LOG REPLICATION
     - Client sends command to Leader
     - Leader appends to its log, sends AppendEntries to all Followers
     - Once majority acknowledge → entry is COMMITTED
     - Leader tells Followers to apply committed entries

  3. SAFETY
     - Election restriction: only nodes with up-to-date logs can win
     - Committed entries are never lost (majority guarantees overlap)

  ┌─────────┐     ┌─────────┐     ┌─────────┐
  │ Leader  │────►│Follower │     │Follower │
  │ (node 0)│────►│(node 1) │     │(node 2) │
  │         │────►│         │     │(crashed) │
  └─────────┘     └─────────┘     └─────────┘
  AppendEntries    ACK             no response
  2 out of 3 = majority → COMMIT

Key structs:
  RaftNode { id, role, current_term, voted_for, log, commit_index, state }
  LogEntry { term, command }
  Role: Follower | Candidate | Leader

Used by: etcd, CockroachDB, TiKV, Consul
```

## 2. Paxos — The Original Consensus Protocol

**File:** `src/paxos.rs`

**Problem:** Same as Raft (agree on a value), but the OG algorithm. Harder to understand, more general.

```
Two phases:
  Phase 1 — PREPARE
    Proposer picks unique number N
    Sends Prepare(N) to all Acceptors
    Acceptor: if N > highest_promised → promise to not accept < N
              if already accepted a value → return it (proposer MUST adopt it)

  Phase 2 — ACCEPT
    If majority promised:
      Proposer sends Accept(N, value) to all Acceptors
      Acceptor: if N >= highest_promised → accept it
    If majority accepted → VALUE IS DECIDED

  ┌──────────┐   Prepare(5)    ┌───────────┐
  │ Proposer │───────────────►│ Acceptor 0 │ → Promise(OK)
  │ (N=5)    │───────────────►│ Acceptor 1 │ → Promise(OK, prev=(3,"X"))
  │          │───────────────►│ Acceptor 2 │ → Promise(OK)
  │          │                └───────────┘
  │          │   Accept(5,"X")  ← MUST adopt previously accepted "X"
  │          │───────────────►  majority accept → DECIDED: "X"
  └──────────┘

  Dueling proposers: two proposers keep outbidding each other's N
  → livelock risk. Solution: Multi-Paxos elects a stable leader.

Key structs:
  Proposer { id, proposal_counter, num_proposers }
  Acceptor { id, highest_promised, accepted: Option<Proposal> }
  Proposal { number, value }

Used by: Google Chubby, Spanner (Multi-Paxos variant)
```

## 3. Two-Phase Commit (2PC) — Distributed Transactions

**File:** `src/two_phase_commit.rs`

**Problem:** Multiple databases need to commit or abort a transaction atomically. No partial commits.

```
Phase 1 — PREPARE (voting)
  Coordinator asks all participants: "Can you commit?"
  Each participant: checks constraints, writes to WAL, votes Yes/No

Phase 2 — COMMIT/ABORT (decision)
  If ALL voted Yes → Coordinator says "Commit"
  If ANY voted No  → Coordinator says "Abort"

  ┌──────────────┐    "Can you commit?"     ┌──────────────┐
  │ Coordinator  │──────────────────────────►│ Participant A │ → Yes (wrote WAL)
  │              │──────────────────────────►│ Participant B │ → Yes (wrote WAL)
  │              │──────────────────────────►│ Participant C │ → No  (insufficient funds)
  │              │                           └──────────────┘
  │              │    ALL must be Yes. C said No.
  │              │──── "ABORT" ────────────►  All participants rollback.
  └──────────────┘

  The problem: if coordinator crashes between Phase 1 and Phase 2,
  participants are stuck with locks. BLOCKING protocol.
  → 3PC adds a pre-commit phase (non-blocking but more messages).
  → Saga pattern avoids the problem entirely (no locks).

Key structs:
  Coordinator { participants }
  Participant { name, data, prepared, will_fail }

Used by: MySQL XA transactions, distributed databases
```

## 4. Saga Pattern — Long-Lived Distributed Transactions

**File:** `src/saga.rs`

**Problem:** 2PC holds locks for the entire transaction. For long-running flows (book flight → hotel → payment), locks are impractical. Saga uses compensating transactions instead.

```
Forward execution:
  Step 1: Reserve flight    ✓
  Step 2: Reserve hotel     ✓
  Step 3: Charge payment    ✗ FAILED
  Step 4: Send confirmation (never reached)

Backward compensation:
  Compensate Step 2: Cancel hotel reservation
  Compensate Step 1: Cancel flight reservation
  → System back to consistent state. No locks were held.

  ┌────────┐   ┌────────┐   ┌────────┐   ┌────────┐
  │Reserve │──►│Reserve │──►│Charge  │   │ Send   │
  │Flight  │   │Hotel   │   │Payment │   │ Email  │
  │   ✓    │   │   ✓    │   │   ✗    │   │        │
  └───┬────┘   └───┬────┘   └────────┘   └────────┘
      │            │         FAIL! Compensate backwards:
      │◄───────────┘         Cancel hotel
      │◄── Cancel flight
      DONE (rolled back)

Key difference from 2PC:
  2PC:  lock all resources → vote → commit/abort (blocking, strong)
  Saga: do each step → if fail, undo in reverse (non-blocking, eventual)

Key structs:
  SagaOrchestrator { steps, state }
  SagaStep { name, will_fail, executed, compensated }

Used by: microservices architectures, e-commerce order flows
```

## 5. Lamport Clocks — Total Event Ordering

**File:** `src/lamport_clock.rs`

**Problem:** No global clock in distributed systems. How to order events?

```
Rules:
  1. Before each event: C = C + 1
  2. Send message:      C = C + 1, attach C to message
  3. Receive message:   C = max(C, received_C) + 1

  Alice (C=0)          Bob (C=0)           Charlie (C=0)
      │                    │                    │
  C=1 ● write("x")        │                    │
      │────── msg(C=1) ──►│                    │
      │                C=2 ● receive            │
      │                    │────── msg(C=2) ──►│
      │                    │                C=3 ● receive
      │                    │                    │

  Guarantee: if A happened-before B → Lamport(A) < Lamport(B)
  BUT: Lamport(A) < Lamport(B) does NOT mean A happened before B
  → Cannot detect concurrent events. Vector clocks fix this.

Key struct:
  LamportClock { time: u64, node_id: String }
```

## 6. Vector Clocks — Causality + Concurrency Detection

**File:** `src/vector_clock.rs`

**Problem:** Lamport clocks can't tell if two events are concurrent. Vector clocks can.

```
Each node maintains a vector of counters (one per node):

  Alice: [A:1, B:0, C:0]   — Alice did 1 event
  Bob:   [A:0, B:2, C:0]   — Bob did 2 events
  Charlie: [A:0, B:0, C:1] — Charlie did 1 event

Comparison:
  [A:1, B:0] vs [A:0, B:1]  →  CONCURRENT (neither dominates)
  [A:1, B:1] vs [A:1, B:0]  →  first HappenedAfter second

  Concurrent = conflict! Need resolution strategy:
    - Last-writer-wins (LWW): pick higher timestamp, lose one write
    - Multi-value: keep both, let application resolve
    - CRDTs: design data types that auto-merge

Key structs:
  VectorClock { clock: HashMap<String, u64> }
  Node { id, vc, data: HashMap<key, (value, write_clock)> }

Used by: Dynamo (Amazon), Riak
```

## 7. CRDTs — Conflict-Free Replicated Data Types

**File:** `src/crdt.rs`

**Problem:** Concurrent writes → conflicts. CRDTs design data types where merge is always safe — no coordination needed.

```
Implemented types:

  G-Counter (grow-only):
    Each node has its own counter. Total = sum of all.
    Merge = max per node. Commutative + associative + idempotent.
    Node A: {A:3, B:0} + Node B: {A:1, B:5} = {A:3, B:5} → total 8

  PN-Counter (increment + decrement):
    Two G-Counters: P (positive) and N (negative).
    Value = P.total - N.total

  LWW-Register (last-writer-wins):
    Store value + timestamp. Higher timestamp wins on merge.
    Simple but LOSSY — concurrent write with lower timestamp is discarded.

  G-Set (grow-only set):
    Items can only be added. Merge = union. Never conflicts.

Key property: merge(A, merge(B, C)) == merge(merge(A, B), C)
              merge(A, B) == merge(B, A)
              merge(A, A) == A
→ Order doesn't matter. Replicas always converge.

Used by: Redis CRDT, Riak, Figma (real-time collaboration)
```

## 8. Gossip Protocol — Epidemic Dissemination

**File:** `src/gossip.rs`

**Problem:** How to spread information to N nodes without a central coordinator?

```
Each round, every node:
  1. Pick a random peer
  2. Push my state / Pull their state
  3. Merge (keep higher versions)

  Round 0:  Only Node 0 knows the data
  Round 1:  Node 0 tells 1 random peer → 2 nodes know
  Round 2:  2 nodes each tell 1 peer → ~4 nodes know
  Round 3:  ~4 tell 1 each → ~8 know
  ...
  Round ~8: all 10 nodes know (O(log N) convergence)

  ┌───┐  gossip  ┌───┐  gossip  ┌───┐
  │ 0 │─────────►│ 3 │─────────►│ 7 │
  └───┘          └───┘          └───┘
    │              │
    └──► ┌───┐     └──► ┌───┐
         │ 5 │          │ 1 │    ... exponential spread
         └───┘          └───┘

Key properties:
  Convergence: O(log N) rounds
  Fault tolerance: random selection routes around failures
  Scalability: O(1) work per node per round
  Consistency: eventual (not instant)

Key struct:
  GossipNode { id, data: HashMap<key, (value, version)>, known_peers }

Used by: Cassandra (membership), Bitcoin (tx propagation), Consul (health)
```

## 9. Chain Replication — High-Throughput Strong Consistency

**File:** `src/chain_replication.rs`

**Problem:** Quorum-based reads (Raft/Paxos) waste resources — every read contacts a majority. Can we do strong consistency with single-node reads?

```
Nodes form a chain: Head → Middle → ... → Tail

Writes: go to Head, propagate through chain to Tail.
        Committed when Tail processes it.

Reads:  go to Tail ONLY. Always returns latest committed value.
        No quorum needed! Single-node read, strongly consistent.

  Client                                               Client
  WRITE──► ┌──────┐   ┌────────┐   ┌────────┐   ┌──────┐ ◄──READ
           │ Head │──►│Middle 1│──►│Middle 2│──►│ Tail │
           └──────┘   └────────┘   └────────┘   └──────┘
           write       propagate   propagate     commit + serve reads

Trade-offs vs Raft:
  ✓ Reads: 1 node (chain) vs majority (Raft) → higher read throughput
  ✓ Write pipelining: overlapping in-flight writes through chain
  ✗ Write latency: must traverse entire chain
  ✗ Failure recovery: needs external config manager (like ZooKeeper)

Key structs:
  Chain { nodes: Vec<ChainNode>, next_version }
  ChainNode { id, role: Head|Middle|Tail, store, pending }

Used by: HDFS (NameNode chain), Azure Storage, CORFU
```

## 10. Leases & Fencing Tokens — Safe Distributed Locking

**File:** `src/lease.rs`

**Problem:** Distributed locks are unsafe. A node can hold a lock, pause (GC, network), and another node gets the lock. Now BOTH think they have it (split-brain).

```
Lease alone is NOT safe:

  Time ─────────────────────────────────────────────────►
  Client A: [acquire lock]────[GC pause 30s]────[write "A"] ← STALE!
  Client B:                   [acquire lock (A's expired)]──[write "B"]
  Storage:                                       gets "A" after "B" → WRONG

Fix: FENCING TOKENS (monotonically increasing)

  Client A: acquires lock → token=33 → GC pause → writes with token=33
  Client B: acquires lock → token=34 → writes with token=34
  Storage:  sees token=34 from B, then token=33 from A
            33 < 34 → REJECTS A's write. Consistency preserved!

  The storage system must check: incoming_token >= max_seen_token
  If not → reject the write.

Key structs:
  LeaseService { current_token, holder, lease_expiry }
  FencedStorage { data, max_token, write_log }

Used by: ZooKeeper (sequential znodes), etcd (revision numbers)
NOT safely implemented by: Redis Redlock (no fencing)
```

## Quick Reference

```
┌────────────┬──────────────────────┬───────────────┬──────────────────────┐
│ Protocol   │ Purpose              │ Consistency   │ Real-world use       │
├────────────┼──────────────────────┼───────────────┼──────────────────────┤
│ Raft       │ Consensus + log repl │ Strong        │ etcd, CockroachDB    │
│ Paxos      │ Consensus (original) │ Strong        │ Spanner, Chubby      │
│ 2PC        │ Atomic transactions  │ Atomic        │ MySQL XA, databases  │
│ Saga       │ Long-lived tx        │ Eventual      │ Microservices        │
│ Lamport    │ Event ordering       │ Causal (partial)│ Foundation for VCs │
│ Vector     │ Causality + conflict │ Causal        │ Dynamo, Riak         │
│ CRDTs      │ Merge w/o coordination│ Eventual     │ Redis, Figma         │
│ Gossip     │ Epidemic spread      │ Eventual      │ Cassandra, Bitcoin   │
│ Chain Repl │ High-throughput repl │ Strong        │ HDFS, Azure Storage  │
│ Leases     │ Safe dist. locking   │ Prevent split │ ZooKeeper, etcd      │
└────────────┴──────────────────────┴───────────────┴──────────────────────┘
```

### When to Use What

```
Need consensus (leader election, config)?
  → Raft (simpler) or Paxos (more general)

Need distributed transactions across databases?
  → 2PC (strong, blocking) or Saga (eventual, non-blocking)

Need to order events / detect conflicts?
  → Lamport clocks (total order) or Vector clocks (detect concurrency)

Need replicas to converge without coordination?
  → CRDTs (design data types that auto-merge)

Need to spread state to N nodes, no coordinator?
  → Gossip (O(log N) convergence, fault-tolerant)

Need strong consistency + high read throughput?
  → Chain Replication (single-node reads, no quorum)

Need a distributed lock?
  → Lease + Fencing Token (never just a lock — always fence!)
```
