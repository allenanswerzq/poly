# etcd — The Distributed Consensus Key-Value Store

---

## 1. What etcd Is and Why It Exists

```
etcd is a strongly consistent, distributed key-value store.
It's the brain of Kubernetes — every cluster state (pods, services,
configs, secrets) lives in etcd. If etcd dies, your cluster is brain-dead.

The problem it solves:
  In a distributed system, multiple services need to AGREE on shared state:
    - "Who is the current leader?"
    - "What's the current cluster configuration?"
    - "Has this lock been acquired?"

  A regular database can't do this reliably:
    - MySQL with replication: async = stale reads, sync = slow + complex
    - Redis: single-node = SPOF, Sentinel = split-brain risk
    - None of these guarantee LINEARIZABILITY across replicas.

  etcd guarantees: if a write returns success, EVERY subsequent read
  from ANY node sees that write. Period. This is linearizability —
  the strongest consistency model possible.

History:
  2013  CoreOS creates etcd for distributed Linux config (etcd = "etc distributed")
  2014  Kubernetes chooses etcd as its backing store
  2015  etcd v2 — HTTP/JSON API, in-memory store
  2018  etcd v3 — gRPC API, MVCC storage, watch improvements
  2018  CNCF incubation project (alongside Kubernetes)
  2020  CNCF graduated project
  2024  etcd v3.5 — stable, production-grade

Who uses it:
  Kubernetes (every cluster), CoreDNS, Vitess, Rook, M3DB, TiKV (PD),
  every service mesh and orchestration system that needs consensus.
```

---

## 2. Core Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     etcd Cluster (typically 3 or 5 nodes)               │
│                                                                         │
│   ┌─────────────┐      ┌─────────────┐      ┌─────────────┐           │
│   │   Node 1    │      │   Node 2    │      │   Node 3    │           │
│   │   LEADER    │◄────►│  FOLLOWER   │◄────►│  FOLLOWER   │           │
│   │             │      │             │      │             │           │
│   │  ┌───────┐  │      │  ┌───────┐  │      │  ┌───────┐  │           │
│   │  │ Raft  │  │      │  │ Raft  │  │      │  │ Raft  │  │           │
│   │  │ Log   │  │      │  │ Log   │  │      │  │ Log   │  │           │
│   │  ├───────┤  │      │  ├───────┤  │      │  ├───────┤  │           │
│   │  │ WAL   │  │      │  │ WAL   │  │      │  │ WAL   │  │           │
│   │  ├───────┤  │      │  ├───────┤  │      │  ├───────┤  │           │
│   │  │ MVCC  │  │      │  │ MVCC  │  │      │  │ MVCC  │  │           │
│   │  │ Store │  │      │  │ Store │  │      │  │ Store │  │           │
│   │  │(bbolt)│  │      │  │(bbolt)│  │      │  │(bbolt)│  │           │
│   │  └───────┘  │      │  └───────┘  │      │  └───────┘  │           │
│   └─────────────┘      └─────────────┘      └─────────────┘           │
│                                                                         │
│   ALL writes go through the Leader.                                    │
│   Reads can go to any node (linearizable or serializable).             │
│   Raft guarantees all nodes see the same log in the same order.        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘

The stack inside each node:

  gRPC API layer        ← client-facing (put, get, watch, lease, txn)
       │
  Raft consensus        ← replicates log entries across nodes
       │
  WAL (Write-Ahead Log) ← durability (append-only file on disk)
       │
  MVCC store            ← multi-version key-value storage
       │
  bbolt (B+ tree)       ← on-disk sorted key-value engine
```

---

## 3. How Raft Works Inside etcd

### 3.1 Normal Write Path

```
Client: PUT /mykey = "hello"
  │
  ▼
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│ Step 1: Client sends request to ANY node via gRPC                   │
│   If the node is a follower, it PROXIES to the leader.              │
│   (Client can also use the leader endpoint directly.)               │
│                                                                      │
│ Step 2: Leader appends entry to its Raft log                        │
│   Log entry = {index: 42, term: 5, data: "PUT mykey=hello"}        │
│   NOT yet applied to the key-value store.                           │
│                                                                      │
│ Step 3: Leader writes entry to its WAL (fsync to disk)              │
│   If leader crashes now, it can recover from WAL on restart.        │
│                                                                      │
│ Step 4: Leader sends AppendEntries RPC to all followers             │
│   Leader → Follower 2: "append {index:42, term:5, PUT mykey=hello}"│
│   Leader → Follower 3: "append {index:42, term:5, PUT mykey=hello}"│
│                                                                      │
│ Step 5: Followers append to their log + WAL, send ACK              │
│   Follower 2 → Leader: "OK, I have index 42"                       │
│   Follower 3 → Leader: "OK, I have index 42"                       │
│                                                                      │
│ Step 6: Leader sees MAJORITY (2 of 3) have the entry               │
│   Leader tracks matchIndex for each follower:                       │
│     matchIndex[leader]    = 42                                      │
│     matchIndex[follower2] = 42  (ACKed)                             │
│     matchIndex[follower3] = 40  (hasn't ACKed yet, doesn't matter) │
│   Sort: [40, 42, 42] → middle value = 42 → majority has it!       │
│   Entry is now COMMITTED. Leader advances commitIndex to 42.       │
│   commitIndex is NOT written to disk — it's derived from           │
│   matchIndex and reconstructed after crash via election.            │
│                                                                      │
│ Step 7: Leader applies entry to MVCC store (bbolt B+ tree)         │
│   mykey now has value "hello" at revision 42.                       │
│                                                                      │
│ Step 8: Leader returns success to client                            │
│                                                                      │
│ Step 9: Followers learn of new commit index via next heartbeat      │
│   They apply the entry to their own MVCC store.                     │
│                                                                      │
│ LATENCY: ~2-5ms (1 network RTT to followers + 2 fsyncs)            │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.2 Leader Election

```
Leader sends heartbeats every 100-150ms. If a follower doesn't receive
a heartbeat for the ELECTION TIMEOUT (1000-2000ms), it starts an election.

┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│  Leader (Node 1) crashes at time T.                                 │
│                                                                      │
│  T + 0ms:     Node 2 and Node 3 stop receiving heartbeats.         │
│                                                                      │
│  T + 1200ms:  Node 2's election timer fires first (randomized).    │
│               Node 2 becomes CANDIDATE.                              │
│               Increments term: 5 → 6                                 │
│               Votes for itself.                                      │
│               Sends RequestVote to Node 3.                          │
│                                                                      │
│  T + 1201ms:  Node 3 receives RequestVote.                          │
│               "Is Node 2's log at least as up-to-date as mine?"     │
│               Yes → grants vote.                                     │
│                                                                      │
│  T + 1202ms:  Node 2 has 2/3 votes → becomes LEADER for term 6.   │
│               Starts sending heartbeats.                             │
│               Replicates any uncommitted entries from its log.       │
│                                                                      │
│  Total downtime: ~1-2 seconds.                                      │
│                                                                      │
│  If Node 1 comes back:                                               │
│    It sees term 6 > its last term 5.                                │
│    Steps down to FOLLOWER.                                          │
│    Receives log from new leader, catches up.                        │
│                                                                      │
│  WHY randomized timeouts:                                            │
│    If Node 2 and Node 3 both timeout at the same moment,           │
│    they both become candidates, both vote for themselves,           │
│    neither gets majority → SPLIT VOTE. Increment term, retry.      │
│    Randomization makes simultaneous candidacy unlikely.             │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.3 Crash Scenarios — What Happens When the Leader Dies Mid-Write

```
The key question: leader sent AppendEntries then crashed.
Some followers got it, some didn't. What happens?

There are 3 possible scenarios. ALL are safe because of Raft's rules.
```

```
SCENARIO 1: Leader crashes BEFORE majority ACKs → entry NOT committed
             → entry MAY or MAY NOT survive (and that's OK!)

  Timeline:
    Leader: append index=42, write WAL, send AppendEntries → CRASH!
    Follower 2: received index=42, wrote to WAL
    Follower 3: did NOT receive (network was slow)

  State after crash:
    Leader (dead): has index=42 in WAL
    Follower 2:    has index=42 in WAL
    Follower 3:    does NOT have index=42

  Election happens. Two sub-cases:

  Case 1a: Follower 2 wins election (has index=42)
    New leader has index=42 in its log.
    But it does NOT know if this entry was committed by the old leader.
    It uses the NO-OP trick to commit it indirectly:

    Step 1: New leader (F2, now term=6) appends a NO-OP entry
      Log: [..., index=42 (term=5, old), index=43 (term=6, NO-OP)]

    Step 2: Replicate both entries to Follower 3
      F3 receives index=42 and index=43, writes to WAL, ACKs.

    Step 3: Majority (F2 + F3) has index=43 (term=6)
      → index=43 is COMMITTED.
      → Committing index=43 also commits everything before it (index=42).
      → Entry SURVIVES.

    Why not just count replicas for index=42 directly?
      Raft rule: a new leader NEVER commits old-term entries by counting.
      Only commits entries from its OWN term (the NO-OP in term=6).
      Old entries ride along — committed as a side effect.
      This prevents a subtle safety bug (Raft paper, Figure 8).

    Client can retry and will see the entry was applied.

  Case 1b: Follower 3 wins election (does NOT have index=42)
    New leader does NOT have index=42.
    New leader sends its log to Follower 2.
    Follower 2 truncates index=42 from its log (overwrites with leader's log).
    → Entry is LOST.
    → This is OK! Leader never told the client "committed."
    → Client got no response (leader crashed) → client retries.
    → Retry creates a NEW entry with a new index. Works fine.

  ┌──────────────────────────────────────────────────────────────┐
  │ IMPORTANT: The client never got a success response.           │
  │ From the client's perspective, the write might or might not   │
  │ have happened → client MUST retry.                            │
  │ This is why etcd operations should be IDEMPOTENT.             │
  │ (PUT the same key=value again → same result.)                │
  └──────────────────────────────────────────────────────────────┘
```

```
SCENARIO 2: Leader crashes AFTER majority ACKs → entry IS committed
             → entry ALWAYS survives (guaranteed!)

  Timeline:
    Leader: append index=42, send AppendEntries
    Follower 2: ACK (has index=42 in WAL)
    Follower 3: ACK (has index=42 in WAL)
    Leader: sees majority → commitIndex=42 → CRASH before replying to client!

  State after crash:
    Leader (dead): has index=42 (committed)
    Follower 2:    has index=42
    Follower 3:    has index=42

  Election happens:
    WHOEVER wins (F2 or F3), they BOTH have index=42.
    Raft's election rule: only a node with the most up-to-date log can win.
    Both F2 and F3 have index=42 → either can win.
    New leader has index=42 → it's committed → it WILL be applied.
    → Entry SURVIVES. Guaranteed. Even though leader crashed.

  But the client didn't get a response (leader crashed before step 8).
  → Client retries. New leader has the entry. Client sees it exists.

  ┌──────────────────────────────────────────────────────────────┐
  │ This is the CORE SAFETY GUARANTEE of Raft:                    │
  │                                                               │
  │ Once a majority has an entry in their logs,                  │
  │ ANY future leader MUST also have that entry.                  │
  │                                                               │
  │ Why? Because to win an election, you need majority votes.    │
  │ A voter only votes for candidates with logs at least as      │
  │ up-to-date as theirs. Since a majority already has the entry,│
  │ any majority of voters OVERLAPS with the majority that has   │
  │ the entry → at least one voter will refuse a candidate       │
  │ that's missing the entry → that candidate can't win.         │
  │                                                               │
  │  Majority who has entry:     {F2, F3}                        │
  │  Majority needed for election: 2 of 3                        │
  │  Any set of 2 includes at least one of {F2, F3}             │
  │  → candidate without entry can't get 2 votes → can't win    │
  └──────────────────────────────────────────────────────────────┘
```

```
SCENARIO 3: Leader crashes AFTER replying to client → everything is fine

  Timeline:
    Leader: commit → apply to bbolt → reply "OK" to client → CRASH

  Client got "OK" → the entry is committed and on a majority.
  New election → new leader has the entry → continues normally.
  No data loss. No inconsistency. Client doesn't even need to retry.
```

```
Summary of crash scenarios:

  ┌────────────────────────────┬──────────────────┬─────────────────────┐
  │ Leader crashes...          │ Entry committed? │ What happens         │
  ├────────────────────────────┼──────────────────┼─────────────────────┤
  │ Before sending to          │ NO               │ Entry lost.          │
  │ followers                  │                  │ Client got no reply, │
  │                            │                  │ retries.             │
  ├────────────────────────────┼──────────────────┼─────────────────────┤
  │ After some followers got   │ MAYBE            │ Depends on who wins  │
  │ it, but not majority       │                  │ election. Entry may  │
  │                            │                  │ survive or be lost.  │
  │                            │                  │ Client retries.      │
  ├────────────────────────────┼──────────────────┼─────────────────────┤
  │ After majority ACKed       │ YES              │ Entry guaranteed to  │
  │ (committed)                │                  │ survive. New leader  │
  │                            │                  │ will have it.        │
  │                            │                  │ Client may retry     │
  │                            │                  │ (no response) but    │
  │                            │                  │ entry is safe.       │
  ├────────────────────────────┼──────────────────┼─────────────────────┤
  │ After replying to client   │ YES              │ Everything fine.     │
  │                            │                  │ No action needed.    │
  └────────────────────────────┴──────────────────┴─────────────────────┘

  The rule: committed = on a majority of logs = PERMANENT.
  Not committed = might be lost = client must retry.
  Client never sees a false "OK" — if they got "OK", the entry is committed.
```

---

## 4. MVCC — Multi-Version Concurrency Control

MVCC in etcd is simpler than in a relational database (no concurrent
transactions competing for the same rows), but the core idea is identical:
**never overwrite data — create new versions, let readers pick which version to see.**

### 4.0 Where the Revision Number Comes From

```
The revision is just a COUNTER on the leader. Not a timestamp.
Not from an external service. Just: revision++.

  Leader has: next_revision = 1

  Client A: PUT /name = "alice"   → leader assigns revision 1, next_revision = 2
  Client B: PUT /count = "10"     → leader assigns revision 2, next_revision = 3
  Client A: PUT /name = "bob"     → leader assigns revision 3, next_revision = 4

  The client does NOT choose or know the revision beforehand.
  The leader assigns it unilaterally as part of the Raft log entry.
  This works because ALL writes go through ONE leader → no conflicts.

Write flow (client's perspective):

  Client ──► etcd: "PUT /name = alice"      (no revision in request)
                        │
  etcd leader:          │
    revision = current + 1 = 42
    create log entry {index:42, data: PUT /name=alice}
    replicate via Raft → commit → apply to bbolt
                        │
  etcd ──► Client: "OK, header: { revision: 42 }"
                        ↑
           Client learns the revision FROM THE RESPONSE.

Read flow (client's perspective):

  1. Default read (latest):
     Client: GET /name
     → Returns latest value + revision in response header.
     → Client doesn't need to know any revision number.

  2. Read at specific revision (time travel):
     Client: GET /name --rev=35
     → Returns value of /name as of revision 35.
     → Client got "35" from a previous response.

  3. Consistent multi-key read:
     resp1 = GET /config/a        → header says revision=50
     resp2 = GET /config/b --rev=50  → reads at SAME point in time
     → Both reads see the same snapshot. Consistent.

  4. Watch from a revision:
     Client: WATCH /pods/ --start-rev=100
     → "Give me all changes since revision 100"
     → Client got "100" from a previous LIST response.

  ┌──────────────────────────────────────────────────────────────┐
  │ Every etcd response includes the current cluster revision:    │
  │                                                               │
  │   PUT response:  { header: { revision: 42 } }               │
  │   GET response:  { value: "alice", mod_revision: 42,         │
  │                    header: { revision: 45 } }                 │
  │   LIST response: { items: [...], header: { revision: 100 } } │
  │                                                               │
  │ No separate "revision service" needed.                       │
  │ The leader IS the revision generator.                        │
  │ Every response carries it.                                   │
  └──────────────────────────────────────────────────────────────┘

How this compares to other systems:
  ┌────────────────────┬──────────────────────────────────────────┐
  │ System             │ How it assigns versions                   │
  ├────────────────────┼──────────────────────────────────────────┤
  │ etcd               │ Leader counter (revision++). Simplest.   │
  │ Spanner            │ TrueTime (GPS + atomic clocks). Real time│
  │ CockroachDB        │ HLC (physical time + logical counter).   │
  │ PostgreSQL         │ Transaction ID (xid), per-node counter.  │
  │ Cassandra          │ Client-side wall-clock timestamp.        │
  │ TiKV (PD)          │ Timestamp Oracle (central server).       │
  └────────────────────┴──────────────────────────────────────────┘

  etcd can use the simplest approach because it has ONE leader.
  If you need multi-leader writes across regions, you need
  real timestamps (HLC/TrueTime) since there's no single counter.
```

```
etcd doesn't overwrite values. Every write creates a NEW REVISION.

  PUT key="name", value="alice"    → revision 1
  PUT key="name", value="bob"      → revision 2
  PUT key="count", value="10"      → revision 3
  PUT key="name", value="carol"    → revision 4

  Internal storage (bbolt B+ tree):

  Key (in bbolt)          Value
  ──────────────────      ──────────────────
  rev 1 → (name)          "alice"
  rev 2 → (name)          "bob"
  rev 3 → (count)         "10"
  rev 4 → (name)          "carol"

  Plus a secondary index (key → list of revisions):
  name  → [rev1, rev2, rev4]    (all revisions for this key)
  count → [rev3]

  GET key="name"          → follow index: name → latest = rev4 → "carol"
  GET key="name" --rev=2  → follow index: name → revisions ≤ 2 → rev2 → "bob"
```

**How etcd's MVCC differs from PostgreSQL's:**

```
  PostgreSQL MVCC:
    - Each ROW has old and new versions (xmin/xmax per tuple)
    - Transactions see a snapshot based on transaction ID
    - Multiple concurrent writers can conflict on the same row
    - Dead versions cleaned by VACUUM

  etcd MVCC:
    - Every write (across ALL keys) increments a GLOBAL revision counter
    - Revision is like a global transaction ID but simpler (just one counter)
    - ALL writes go through one Raft leader → fully serialized, no conflicts
    - Old revisions cleaned by COMPACTION

  Conceptually:
    PostgreSQL:     row-level versions, per-transaction snapshots
    etcd:           cluster-level revisions, global monotonic counter

  etcd can be simpler because Raft serializes all writes through one leader.
  There are no concurrent writers to the same data — every write gets a
  unique, ordered revision number. No conflict detection needed.
```

**Why MVCC matters for etcd specifically:**

```
  1. WATCHES: the killer use case.
     A watch says "give me all changes since revision N."
     Because old revisions exist in storage, etcd can:
       - Stream all changes from revision N to current
       - Then switch to live-streaming new changes
     Without MVCC, watches would need a separate change log.

  2. CONSISTENT READS across multiple keys:
     "Read keys /config/a and /config/b at the SAME point in time."
     By specifying a revision, both reads see the same snapshot.
     No risk of reading /config/a before an update and /config/b after.

  3. HISTORICAL READS for debugging:
     "What was the leader key 5 minutes ago?"
     Read at a past revision (if not yet compacted).
```

### 4.1 Compaction

```
MVCC means revisions accumulate forever. Compaction trims old ones.

  Before compaction (compact at rev 3):
    rev 1 → name="alice"
    rev 2 → name="bob"
    rev 3 → count="10"
    rev 4 → name="carol"

  After compaction:
    rev 3 → count="10"      (kept: latest value at compaction point)
    rev 4 → name="carol"    (kept: after compaction point)
    rev 1, 2 → DELETED

  Kubernetes auto-compacts every 5 minutes.
  If you don't compact, bbolt grows indefinitely → disk full → cluster down.
  This is the #1 etcd operational issue.
```

---

## 5. Watch — etcd's Killer Feature

```
Watches let clients subscribe to key changes in real-time.
This is how Kubernetes controllers work: they WATCH for changes
and react, instead of polling.

  Client A: WATCH key="/pods/" (prefix)

  (sometime later)

  Client B: PUT /pods/nginx-123 = '{"status": "running"}'

  Client A receives: WatchEvent {
    type: PUT,
    key: "/pods/nginx-123",
    value: '{"status": "running"}',
    revision: 4892
  }

How it works internally:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Every watch is registered with a START REVISION.           │
  │                                                              │
  │  When a new entry is committed:                             │
  │    1. Applied to MVCC store (creates new revision)          │
  │    2. Watch hub checks: does any watch match this key?      │
  │    3. If yes → push event to that client's gRPC stream      │
  │                                                              │
  │  If a watch reconnects (network blip):                      │
  │    Client says "resume from revision 4890"                  │
  │    etcd reads revisions 4890..current from MVCC store       │
  │    Sends all missed events, then switches to live stream    │
  │                                                              │
  │  This is why MVCC + compaction interact:                    │
  │    If client was disconnected for 2 hours, and compaction   │
  │    deleted revisions before their resume point:             │
  │    → etcd returns "compacted" error                         │
  │    → client must re-list all keys and start a new watch     │
  │    (This is Kubernetes "relist" — expensive but rare.)      │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Kubernetes pattern: List + Watch

    1. LIST /pods/ → get all pods + the current resource version (= etcd revision)
    2. WATCH /pods/ starting from that revision → get all future changes
    3. If watch disconnects → re-LIST and re-WATCH

    This is eventually consistent but with strong ordering guarantees.
    Every event is delivered in revision order. No events are missed
    (unless compacted, then re-list).
```

---

## 6. Transactions (Mini-Transactions)

```
etcd supports compare-and-swap style transactions:

  "IF key X has value V, THEN set key Y to W, ELSE set key Y to Z"

  Txn(
    // IF (compare)
    Compare: [value("leader") == "node-1"],
    // THEN (success)
    Success: [put("leader", "node-2")],
    // ELSE (failure)
    Failure: [get("leader")]
  )

  This is ATOMIC — the compare + action happens as one operation.
  No other write can sneak in between the compare and the action.

  USE CASES:
  - Distributed locks: "if lock doesn't exist, create it"
  - Leader election: "if no leader, I'm the leader"
  - CAS (compare-and-swap): optimistic concurrency
  - Kubernetes: "if resourceVersion matches, apply update"

  LIMITATION: etcd txns are single-round. No multi-step sagas.
  For complex workflows, you need something like FoundationDB.
```

---

## 7. Leases — TTL-Based Keys

```
A lease is a time-to-live (TTL) attached to one or more keys.
When the lease expires, ALL keys attached to it are deleted.

  Client creates lease: TTL = 10 seconds → lease ID = 123
  Client puts key with lease: PUT /services/my-app, lease=123

  Client must keep-alive the lease (heartbeat every few seconds).
  If client crashes → stops heartbeating → lease expires → key deleted.

  ┌──────────────────────────────────────────────────────────────┐
  │  This is how SERVICE DISCOVERY works:                       │
  │                                                              │
  │  Service registers:                                         │
  │    PUT /services/my-app/instance-1 = "10.0.1.5:8080"       │
  │    with lease TTL = 30 seconds                              │
  │    keep-alive every 10 seconds                              │
  │                                                              │
  │  Service crashes:                                           │
  │    Keep-alive stops → 30 seconds later → key deleted        │
  │    Watchers see DELETE event → remove from load balancer    │
  │                                                              │
  │  This is ephemeral node behavior — same concept as          │
  │  ZooKeeper ephemeral znodes.                                │
  └──────────────────────────────────────────────────────────────┘
```

---

## 8. Why Only 3 or 5 Nodes?

```
etcd is NOT designed for large data or large clusters.

  3 nodes: tolerates 1 failure (majority = 2)
  5 nodes: tolerates 2 failures (majority = 3)
  7 nodes: tolerates 3 failures (majority = 4)

  WHY NOT more?
    Every write must be replicated to MAJORITY before commit.
    More nodes = more network round trips = higher write latency.
    7+ nodes rarely makes sense — the marginal fault tolerance
    isn't worth the latency cost.

  WHY odd numbers?
    3 nodes: majority = 2, tolerate 1 failure
    4 nodes: majority = 3, tolerate 1 failure  ← same tolerance, worse performance!
    5 nodes: majority = 3, tolerate 2 failures

    4 nodes buys you NOTHING over 3 nodes for fault tolerance
    but costs more write latency (3 confirmations vs 2).
    Always use odd numbers.

  Data size limits:
    Recommended max: ~8 GB database size
    Kubernetes clusters rarely exceed 2-4 GB in etcd
    etcd is for METADATA — config, coordination, small state
    NOT for application data. Use DynamoDB/Cassandra for that.
```

---

## 9. Operational Reality — Why etcd Breaks

```
┌──────────────────────────────────────────────────────────────────┐
│ Problem                   │ Cause                   │ Fix        │
├───────────────────────────┼─────────────────────────┼────────────┤
│ Disk full                 │ No compaction / too many│ Auto-compact│
│                           │ revisions               │ every 5min │
├───────────────────────────┼─────────────────────────┼────────────┤
│ Slow writes               │ Disk I/O latency        │ Use SSDs!  │
│                           │ (WAL fsync is critical) │ Never HDD. │
├───────────────────────────┼─────────────────────────┼────────────┤
│ Leader flapping           │ Network instability or  │ Tune       │
│                           │ CPU contention          │ heartbeat/ │
│                           │                         │ election   │
│                           │                         │ timeouts   │
├───────────────────────────┼─────────────────────────┼────────────┤
│ DB size alarm             │ Exceeded 8 GB           │ Defragment │
│ (cluster stops writes)    │                         │ + compact  │
├───────────────────────────┼─────────────────────────┼────────────┤
│ Split brain               │ Network partition with  │ Can't      │
│ (shouldn't happen w/Raft) │ misconfig (wrong URLs)  │ happen if  │
│                           │                         │ configured │
│                           │                         │ correctly  │
├───────────────────────────┼─────────────────────────┼────────────┤
│ Watch storm               │ Too many watchers or    │ Reduce     │
│ (high CPU)                │ too frequent changes    │ watch count│
└───────────────────────────┴─────────────────────────┴────────────┘

THE GOLDEN RULES:
  1. Use SSDs. etcd fsync's on every write. HDD = dead cluster.
  2. Auto-compact. Without it, bbolt grows until disk is full.
  3. Defragment periodically. Compaction marks space as free but
     bbolt doesn't return it to the OS until defrag.
  4. Monitor disk_wal_fsync_duration. If > 10ms, your disks are too slow.
  5. Keep etcd on dedicated nodes (don't colocate with heavy workloads).
  6. Backup regularly: etcdctl snapshot save.
```

---

## 10. etcd vs. ZooKeeper vs. Consul

```
┌──────────────────┬─────────────────┬──────────────────┬─────────────┐
│                  │ etcd            │ ZooKeeper        │ Consul      │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Consensus        │ Raft            │ ZAB (Zab atomic  │ Raft        │
│                  │                 │ broadcast)       │             │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ API              │ gRPC + HTTP     │ Custom TCP       │ HTTP + DNS  │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Data model       │ Flat KV +       │ Hierarchical     │ KV +        │
│                  │ prefix ranges   │ tree (znodes)    │ service     │
│                  │                 │                  │ catalog     │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Watch            │ Revision-based  │ One-time trigger │ Blocking    │
│                  │ streaming       │ (must re-watch)  │ queries     │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Transactions     │ Mini-txn (CAS)  │ Multi-op         │ CAS only    │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Linearizable     │ Yes (default)   │ Yes              │ Optional    │
│ reads            │                 │                  │             │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Used by          │ Kubernetes,     │ Kafka (legacy),  │ HashiCorp   │
│                  │ TiKV            │ Hadoop, Solr     │ stack       │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Language         │ Go              │ Java             │ Go          │
├──────────────────┼─────────────────┼──────────────────┼─────────────┤
│ Maturity         │ 2013            │ 2008             │ 2014        │
└──────────────────┴─────────────────┴──────────────────┴─────────────┘

Pick etcd if: you're in the Kubernetes ecosystem or want a modern,
              well-maintained consensus store with gRPC.
Pick ZooKeeper if: you're running Kafka <4.0 (KRaft removed the need).
Pick Consul if: you need service mesh + service discovery + KV in one tool.
```

---

## 11. Key Numbers

```
Write latency:           2-5ms (Raft consensus + fsync)
Read latency:            0.1-1ms (linearizable requires leader)
Serializable read:       <0.1ms (any follower, may be stale)
Throughput:              ~10K-30K writes/sec, ~100K reads/sec
Max database size:       8 GB recommended
Max key size:            1.5 MB
Max value size:          1.5 MB
Max watchers:            ~10K per node (practical)
Election timeout:        1000-2000ms
Heartbeat interval:      100-150ms
Snapshot interval:       every 10,000 applied entries (default)
Recommended cluster:     3 or 5 nodes
```
