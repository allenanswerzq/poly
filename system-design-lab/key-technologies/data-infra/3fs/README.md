# 3FS (Fire-Flyer File System) — DeepSeek's Distributed Filesystem

---

## 1. What 3FS Is and Why DeepSeek Built It

```
3FS is a distributed filesystem built by DeepSeek specifically for
AI training and inference workloads. Open-sourced February 2025.

  Why not just use Lustre?
    1. Lustre's MDS (Metadata Server) is a bottleneck at DeepSeek's scale
    2. Lustre's kernel client is hard to customize (kernel module vs FUSE)
    3. Lustre's RDMA support exists but isn't deeply integrated into
       the data path
    4. DeepSeek wanted chain replication (simpler consistency than
       Lustre's locking model)
    5. Full control over the storage stack for their specific hw

  3FS is designed for:
    - RDMA-native networking (InfiniBand / RoCE)
    - NVMe SSDs (not spinning disks)
    - AI workloads: large sequential reads (training data),
      large sequential writes (checkpoints),
      random reads (KV cache for inference)

  Open-source: github.com/deepseek-ai/3FS (Apache 2.0)
  Written in: C++ (core), Rust (some components)
```

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                          3FS CLUSTER                                 │
│                                                                      │
│  METADATA CLUSTER (3 nodes, Raft consensus)                         │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                                                                │ │
│  │  Meta Node 1 ◄──── Raft ────► Meta Node 2                    │ │
│  │       ▲                            ▲                           │ │
│  │       └──────── Raft ──────────────┘                           │ │
│  │                    │                                            │ │
│  │              Meta Node 3                                       │ │
│  │                                                                │ │
│  │  Stores:                                                       │ │
│  │    - Directory tree (inodes, dentries)                         │ │
│  │    - File → chunk chain mapping                               │ │
│  │    - Chunk → storage node mapping                             │ │
│  │    - File permissions, sizes, timestamps                      │ │
│  │                                                                │ │
│  │  Why Raft:                                                     │ │
│  │    Strong consistency for metadata.                            │ │
│  │    If meta leader dies, Raft elects new leader in ~1 second.  │ │
│  │    No split-brain. No inconsistent directory listings.         │ │
│  │                                                                │ │
│  │  Why only 3 nodes:                                             │ │
│  │    Metadata is small (KB per file). 3 nodes handles millions  │ │
│  │    of files easily. Not a bottleneck for AI workloads          │ │
│  │    (large files, not millions of small ones).                 │ │
│  │                                                                │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  STORAGE NODES (100s of nodes, each with NVMe SSDs)                │
│  ┌──────────────┐  ┌──────────────┐      ┌──────────────┐         │
│  │ Storage 0    │  │ Storage 1    │      │ Storage N    │         │
│  │              │  │              │      │              │         │
│  │ NVMe SSDs    │  │ NVMe SSDs    │      │ NVMe SSDs    │         │
│  │ [disk0]      │  │ [disk0]      │      │ [disk0]      │         │
│  │ [disk1]      │  │ [disk1]      │      │ [disk1]      │         │
│  │ [disk2]      │  │ [disk2]      │      │ [disk2]      │         │
│  │ ...          │  │ ...          │      │ ...          │         │
│  │              │  │              │      │              │         │
│  │ RDMA NIC     │  │ RDMA NIC     │      │ RDMA NIC     │         │
│  │ (ConnectX-7) │  │ (ConnectX-7) │      │ (ConnectX-7) │         │
│  └──────────────┘  └──────────────┘      └──────────────┘         │
│                                                                      │
│  CLIENTS (every compute/GPU node)                                   │
│  ┌──────────────┐  ┌──────────────┐      ┌──────────────┐         │
│  │ Client 0     │  │ Client 1     │      │ Client M     │         │
│  │              │  │              │      │              │         │
│  │ FUSE mount   │  │ FUSE mount   │      │ FUSE mount   │         │
│  │ /mnt/3fs     │  │ /mnt/3fs     │      │ /mnt/3fs     │         │
│  │              │  │              │      │              │         │
│  │ RDMA reads   │  │ RDMA reads   │      │ RDMA reads   │         │
│  │ direct to    │  │ direct to    │      │ direct to    │         │
│  │ storage nodes│  │ storage nodes│      │ storage nodes│         │
│  │              │  │              │      │              │         │
│  │ 8× GPUs      │  │ 8× GPUs      │      │ 8× GPUs      │         │
│  └──────────────┘  └──────────────┘      └──────────────┘         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Chain Replication — How Data Is Written

```
3FS uses CHAIN REPLICATION instead of Raft or primary-backup for data.

  Each chunk is replicated across a CHAIN of 3 storage nodes:

    Write path:
    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │  Client writes chunk X:                                    │
    │                                                             │
    │  Client ──► Head (Node A) ──► Middle (Node B) ──► Tail (C) │
    │                                                      │      │
    │                                              ACK ◄───┘      │
    │                                                             │
    │  1. Client sends data to HEAD of the chain                 │
    │  2. Head writes to local NVMe, forwards to Middle          │
    │  3. Middle writes to local NVMe, forwards to Tail          │
    │  4. Tail writes to local NVMe, sends ACK back to Client   │
    │                                                             │
    │  Data is COMMITTED only when Tail ACKs.                    │
    │  Strong consistency: if Tail has it, everyone has it.      │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘

  Why chain replication instead of Raft for data:

    Raft:
      Leader sends to all followers in parallel.
      Must wait for majority. Voting overhead per write.
      Good for: small metadata (KB).
      Overhead is ~2× baseline (leader writes + waits).

    Chain replication:
      Data flows HEAD → MIDDLE → TAIL sequentially.
      No voting. Simpler protocol. Pipeline parallelism.
      While chunk N flows A→B, chunk N+1 flows Client→A.
      Good for: large data chunks (MB-GB).
      Throughput: nearly matches single-node write speed
      (pipeline hides replication latency).

    ┌────────────────────────────────────────────────────────┐
    │ Time →                                                 │
    │                                                        │
    │ Raft (parallel, but wait for majority):                │
    │   Leader: [write] ─── wait ──── [ACK to client]       │
    │   Foll 1: ──[write]──────ACK──►                       │
    │   Foll 2: ──[write]──────ACK──►                       │
    │   Latency: 1 write + 1 RTT                            │
    │                                                        │
    │ Chain (sequential, but pipelined):                     │
    │   Chunk 1: [A write][B write][C write][ACK]           │
    │   Chunk 2:     [A write][B write][C write][ACK]       │
    │   Chunk 3:         [A write][B write][C write][ACK]   │
    │   Latency per chunk: 3 writes. But pipelined!         │
    │   Throughput: ~same as single-node after pipeline fills│
    │                                                        │
    └────────────────────────────────────────────────────────┘
```

---

## 4. CRAQ — How Reads Are Distributed

```
Standard chain replication: reads ONLY from Tail.
  Tail is guaranteed to have the latest committed version.
  But: Tail becomes a read bottleneck.

3FS uses CRAQ (Chain Replication with Apportioned Queries):

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Each node in the chain tracks:                             │
  │    - committed version (known to be on all nodes)           │
  │    - pending version (written here but not yet ACK'd by Tail)│
  │                                                              │
  │  Read from ANY node:                                        │
  │                                                              │
  │    Case 1: Node has ONLY committed version (no pending)     │
  │      → Respond immediately. This is the latest.             │
  │      → No coordination needed. Very fast.                   │
  │                                                              │
  │    Case 2: Node has a PENDING version                       │
  │      → "I might have a newer version, but I'm not sure      │
  │         if Tail has committed it yet."                       │
  │      → Ask Tail: "is version X committed?"                  │
  │      → Tail responds: YES → respond with version X.         │
  │                        NO → respond with committed version. │
  │                                                              │
  │  Result:                                                    │
  │    - When writes are rare: 3 nodes serve reads (3× read    │
  │      throughput vs standard chain rep).                     │
  │    - When writes are active: some reads fall back to Tail   │
  │      check, but most still served locally.                  │
  │    - ALWAYS strongly consistent (never returns stale data). │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  For ML training (mostly reads):
    Training data is written once, read millions of times.
    All nodes in each chain serve reads → near-linear read scaling.
    180 storage nodes × full RDMA bandwidth = 6.2 TB/s aggregate reads.
```

---

## 5. RDMA Data Path — Why It's Fast

```
Traditional filesystem read path:

  App → syscall → kernel VFS → filesystem → page cache → disk driver
  → DMA from disk → kernel buffer → copy to user buffer → App

  Multiple kernel transitions. Multiple memory copies. CPU involved.

3FS RDMA read path:

  App → FUSE request → 3FS client (userspace)
    → RDMA READ to storage node (one-sided, no CPU on storage!)
    → Data arrives directly in client's memory buffer
    → returned to App

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  CLIENT                              STORAGE NODE            │
  │  ┌───────────┐                       ┌───────────┐          │
  │  │           │                       │           │          │
  │  │ App calls │                       │ NVMe SSD  │          │
  │  │ read()    │                       │ data in   │          │
  │  │     │     │                       │ memory    │          │
  │  │     ▼     │                       │  (cached) │          │
  │  │ FUSE      │                       │     ▲     │          │
  │  │     │     │   RDMA one-sided read │     │     │          │
  │  │     ▼     │   ─────────────────── │     │     │          │
  │  │ 3FS client│──────────────────────►│ NIC does  │          │
  │  │           │◄──────────────────────│ DMA read  │          │
  │  │ data in   │   data arrives in     │ from local│          │
  │  │ user buf  │   client memory via   │ memory    │          │
  │  │           │   RDMA                │           │          │
  │  └───────────┘                       └───────────┘          │
  │                                                              │
  │  Storage node's CPU is NOT involved.                        │
  │  The NIC reads from memory directly via RDMA.               │
  │  This is "one-sided RDMA" — only the initiator's CPU runs.  │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Why this matters for AI:
    - 16K GPUs loading training data simultaneously
    - Each wants ~100 MB/s of read throughput
    - With TCP: storage node CPU is bottleneck (handling 1000s of sockets)
    - With RDMA: storage node CPU is idle. NIC handles everything.
    - Storage node can serve thousands of clients at line rate.
```

---

## 6. File Layout — Chunks and Chains

```
When you write a file to 3FS:

  file: /training/shard_042.bin (10 GB)

  1. Client contacts metadata cluster: "create this file"
  2. Metadata cluster assigns chains for each chunk:

     ┌───────────────────────────────────────────────────────┐
     │ File: /training/shard_042.bin                         │
     │                                                       │
     │ Chunk 0 (64 MB):  chain = [Node 12, Node 45, Node 78]│
     │ Chunk 1 (64 MB):  chain = [Node 33, Node 67, Node 91]│
     │ Chunk 2 (64 MB):  chain = [Node 5,  Node 22, Node 55]│
     │ ...                                                   │
     │ Chunk 159 (64 MB): chain = [Node 88, Node 3, Node 41]│
     │                                                       │
     │ Total: 160 chunks × 64 MB = 10 GB                    │
     │ Each chunk has its own independent 3-node chain.      │
     └───────────────────────────────────────────────────────┘

  3. Client writes each chunk to its chain head.
     Can write multiple chunks in PARALLEL (different chains).
     10 chains writing simultaneously: 10× throughput.

  4. For reads: client can read each chunk from ANY node in
     its chain (CRAQ). 160 chunks × 3 nodes each = up to
     480 read sources for this one file.


  Chain assignment strategy:
    - Spread chains across different storage nodes and racks
    - Balance: each storage node participates in roughly
      equal number of chains
    - Rack-awareness: 3 nodes in a chain span ≥ 2 racks
      (survive single rack failure)
```

---

## 7. Failure Handling

```
Storage node failure:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Chain: [A] → [B] → [C]                                    │
  │                                                              │
  │  Node B dies:                                               │
  │    1. Metadata cluster detects (heartbeat timeout)          │
  │    2. Chain reconfigured: [A] → [C]                        │
  │       (skip B, A now sends directly to C)                   │
  │    3. Reads still work: A and C have the data               │
  │    4. Background: metadata picks new Node D,                │
  │       replicates data from A/C → D                          │
  │    5. Chain becomes: [A] → [D] → [C]                       │
  │    6. Back to 3× replication. No data loss.                │
  │                                                              │
  │  Tail (C) dies:                                             │
  │    Chain: [A] → [B]                                        │
  │    B becomes new Tail. Reads now from B.                    │
  │    Any pending writes that B has but C didn't ACK           │
  │    are now committed (B is the new Tail).                   │
  │                                                              │
  │  Head (A) dies:                                             │
  │    Chain: [B] → [C]                                        │
  │    B becomes new Head. Client writes to B now.             │
  │    Reads still from C (or B via CRAQ).                     │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

Metadata node failure:
  Raft handles this automatically.
  If leader dies → remaining 2 nodes elect new leader (~1 sec).
  Clients reconnect to new leader. No data loss.
  3FS survives 1 metadata node failure (2 of 3 alive = majority).
```

---

## 8. Use Cases at DeepSeek

```
1. TRAINING DATA STORAGE
   15T+ tokens stored as pre-tokenized binary shards.
   Each DP replica reads its shard file sequentially.
   CRAQ distributes reads across all chain nodes.
   Aggregate read: 6.2 TB/s across the cluster.

2. CHECKPOINT WRITES
   Every N steps: all GPUs write their model shard to 3FS.
   Each GPU writes ~500 MB to a different file.
   Writes go to different chains → parallel across storage cluster.
   Chain replication ensures durability before ACK.

3. KV CACHE FOR INFERENCE (with DeepSeek's "KVCache offloading")
   During inference, KV cache entries are stored/retrieved from 3FS.
   Random reads of small chunks → RDMA one-sided reads are ideal.
   Sub-millisecond latency.

4. MODEL ARTIFACT STORAGE
   Model weights, configs, tokenizers.
   Written once, read many times across inference fleet.
   CRAQ means all 3 replicas serve reads → no hotspot.
```

---

## 9. 3FS vs Lustre vs Ceph

```
┌──────────────────┬──────────────┬──────────────┬──────────────┐
│                  │ 3FS          │ Lustre       │ Ceph         │
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Data replication │ Chain rep    │ RAID/rep     │ Primary-     │
│                  │ (CRAQ)       │ (server-side)│ backup       │
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Consistency      │ Strong       │ Strong       │ Strong       │
│                  │ (chain tail) │ (locks)      │ (all ACK)    │
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Metadata         │ Raft (3 nodes│ Central MDS  │ CRUSH (no    │
│                  │ simple)      │ (bottleneck) │ MDS for data)│
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Client           │ FUSE         │ Kernel module│ FUSE or      │
│                  │ (userspace)  │ (fast, hard  │ kernel       │
│                  │              │ to debug)    │              │
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ RDMA support     │ Native       │ Bolt-on      │ Partial      │
│                  │ (core design)│ (LNet layer) │              │
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Read distribution│ All chain    │ Stripe-based │ Primary only │
│                  │ nodes (CRAQ) │ (one copy)   │ (or read     │
│                  │              │              │ from replica)│
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Maturity         │ New (2025)   │ 20+ years    │ 10+ years    │
│                  │ DeepSeek only│ Battle-tested│ Widely used  │
├──────────────────┼──────────────┼──────────────┼──────────────┤
│ Best for         │ RDMA clusters│ HPC, general │ Cloud, mixed │
│                  │ AI-specific  │ purpose      │ workloads    │
└──────────────────┴──────────────┴──────────────┴──────────────┘
```

---

## 10. Key Numbers

```
DeepSeek's reported performance (180 storage nodes):

  Aggregate read throughput:    ~6.2 TB/s
  Aggregate write throughput:   ~3 TB/s
  Single-client read:           saturates RDMA (~100 Gbps = 12.5 GB/s)
  Read latency (cached):        sub-millisecond
  Metadata ops:                 ~100K+ ops/sec (Raft cluster)
  Replication:                  3× (chain replication)
  Chunk size:                   configurable (64 MB typical)
  Network:                      RDMA (InfiniBand or RoCE)
  Storage media:                NVMe SSDs
  Client:                       FUSE (userspace)
```
