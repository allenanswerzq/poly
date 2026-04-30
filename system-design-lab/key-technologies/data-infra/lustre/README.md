# Distributed Filesystems — How Large-Scale Storage Works

---

## 1. The Problem

```
A single disk or single server can't serve thousands of machines:

  Single NFS server:
    Throughput: ~1-5 GB/s
    Capacity: ~100 TB
    Clients: ~50-100 before performance degrades
    Single point of failure

  What large-scale workloads need:
    ML training (16K GPUs):   100+ GB/s reads, 8 TB checkpoint writes
    Data warehouse (Hadoop):  PB-scale storage, thousands of readers
    HPC simulation:           millions of small files, low latency
    Web serving (CDN origin): billions of objects, globally distributed

  The solution: spread data across hundreds of servers.
  The challenge: make it look like ONE filesystem to the user.
```

---

## 2. The Design Space

```
All distributed filesystems make different tradeoffs on:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  1. Interface: POSIX (like a normal filesystem) vs Object    │
  │     POSIX: open/read/write/close, directories, permissions  │
  │     Object: PUT/GET/DELETE by key (like S3)                 │
  │                                                              │
  │  2. Consistency: strong vs eventual                         │
  │     Strong: read always sees latest write                   │
  │     Eventual: reads may be stale for a moment              │
  │                                                              │
  │  3. Optimized for: large files vs small files               │
  │     Large: striping across many servers (throughput)        │
  │     Small: metadata-heavy, many lookups (latency)           │
  │                                                              │
  │  4. Metadata: centralized vs distributed                    │
  │     Centralized: simple, but single bottleneck              │
  │     Distributed: scales better, more complex                │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

---

## 3. Architecture Patterns

```
Almost every distributed filesystem has the same three components:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  CLIENT (runs on every compute node)                            │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  Translates POSIX calls (open, read, write) into          │ │
  │  │  network requests to the metadata + data servers.         │ │
  │  │  Usually a kernel module or FUSE driver.                  │ │
  │  │  Caches metadata and sometimes data locally.              │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │       │                            │                             │
  │       │ "where is file X?"         │ "give me bytes of file X"  │
  │       ▼                            ▼                             │
  │  METADATA SERVER                DATA SERVERS                    │
  │  ┌──────────────────┐          ┌──────────────────────────────┐ │
  │  │                  │          │                              │ │
  │  │  Directory tree   │          │  Server 0: [chunk A, D, G] │ │
  │  │  File → chunk map│          │  Server 1: [chunk B, E, H] │ │
  │  │  Permissions      │          │  Server 2: [chunk C, F, I] │ │
  │  │  File sizes       │          │  ...                       │ │
  │  │  Lock state       │          │  Server N: [chunk ...]     │ │
  │  │                  │          │                              │ │
  │  └──────────────────┘          └──────────────────────────────┘ │
  │                                                                  │
  │  Metadata is SMALL (KB per file) → few servers, fast SSDs.     │
  │  Data is LARGE (GB-TB per file) → many servers, big disks.     │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

The read path:
  1. Client: open("/data/model.bin")
  2. Client → Metadata server: "where is model.bin?"
  3. Metadata server → Client: "chunks are on servers [3, 7, 12, 19]"
  4. Client → Data servers [3, 7, 12, 19]: "give me chunks" (PARALLEL)
  5. Client reassembles chunks → returns data to application

The write path:
  1. Client: write(fd, data, size)
  2. Client → Metadata server: "allocate space for new data"
  3. Metadata server: "write to servers [5, 11, 22]" (picks based on load)
  4. Client → Data servers [5, 11, 22]: "store these chunks" (PARALLEL)
  5. Data servers ACK → Client → Metadata server: "update file size"
```

---

## 4. The Major Systems

### 4.1 Lustre — HPC Parallel Filesystem

```
DESIGNED FOR: maximum throughput for large sequential I/O.
USED BY:      Meta (LLaMA), NVIDIA, national labs, supercomputers.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  MDS (Metadata Server) — 1-4 servers                        │
  │    Stores: directory tree, file-to-OST mapping              │
  │    Backed by: ldiskfs or ZFS on fast SSDs                   │
  │    Can shard directories across multiple MDTs (Lustre 2.4+) │
  │                                                              │
  │  OSS/OST (Object Storage Servers/Targets) — 100s of servers│
  │    Each OSS manages 1-2 OSTs (disk arrays)                  │
  │    Each OST = a local filesystem (ldiskfs/ZFS) on disks     │
  │    Files are STRIPED across multiple OSTs                    │
  │                                                              │
  │  Client — kernel module (mount -t lustre)                   │
  │    POSIX compatible: ls, cat, cp, torch.save all work       │
  │    Talks to MDS for metadata, directly to OSSes for data    │
  │    Network: InfiniBand RDMA (bypasses CPU on data path)     │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  File striping:
    $ lfs setstripe -c 4 -S 1M myfile.bin
    # -c 4: stripe across 4 OSTs
    # -S 1M: each stripe is 1 MB

    myfile.bin (10 MB):
      OST 12: [0-1MB] [4-5MB] [8-9MB]
      OST 37: [1-2MB] [5-6MB] [9-10MB]
      OST 55: [2-3MB] [6-7MB]
      OST 81: [3-4MB] [7-8MB]

    4 OSTs read in parallel → 4× throughput.
    Default stripe count: 1 (for small files).
    For checkpoints: stripe across many OSTs.

  Performance:
    Single client:     ~5-20 GB/s (depends on stripe count and network)
    Aggregate cluster: 100-2000+ GB/s (scales with number of OSSes)
    Typical HPC:       200 OSSes × 10 GB/s each = 2 TB/s

  Weaknesses:
    - Metadata: single MDS was a bottleneck (fixed in newer versions
      with Distributed Namespace / DNE)
    - Small files: terrible performance (1 RPC per file open)
    - Operational complexity: hard to manage, fragile
```

### 4.2 HDFS — Hadoop Distributed File System

```
DESIGNED FOR: batch analytics on commodity hardware.
USED BY:      Hadoop ecosystem (MapReduce, Spark, Hive, Presto).

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  NameNode (metadata) — 1 server (+ standby for HA)         │
  │    Stores: entire directory tree + block locations IN RAM    │
  │    Why RAM: fast lookups. 1B files ≈ 10 GB metadata.       │
  │    Single point of failure → standby NameNode for failover. │
  │                                                              │
  │  DataNodes (data) — 100s to 1000s of servers               │
  │    Each stores blocks (default 128 MB each)                 │
  │    Each block replicated 3× across different racks          │
  │    Heartbeat to NameNode every 3 seconds                    │
  │                                                              │
  │  Client — Java library (no kernel mount)                    │
  │    NOT POSIX. Custom API: FileSystem.open(), .create()      │
  │    Write-once: files are append-only after creation          │
  │    No random writes. No in-place modification.              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Write path:
    1. Client → NameNode: "create /data/file.parquet"
    2. NameNode: "write block 1 to DataNodes [5, 23, 47]"
    3. Client → DN5: sends block (128 MB)
       DN5 → DN23: replicates (pipeline)
       DN23 → DN47: replicates
    4. All 3 DNs ACK → block is committed
    5. Next block: NameNode picks different DataNodes

  Key design choices:
    - Write-once, append-only: simplifies consistency
    - 128 MB blocks: optimized for large sequential reads (analytics)
    - 3× replication: simple, but 3× storage cost
      (newer HDFS supports erasure coding: 1.5× cost, same durability)
    - Rack-awareness: replicas on different racks for fault tolerance
    - Data locality: Spark/MapReduce tries to compute on the node
      where the data lives → no network transfer needed

  Performance:
    Single stream: ~100-200 MB/s (limited by 1 Gbps Ethernet era)
    Aggregate: scales with DataNodes (100s of GB/s total)
    Latency: ~10-50ms first byte (not for real-time)

  Weaknesses:
    - Small files: each file = 1 NameNode entry = ~150 bytes RAM.
      1 billion small files → 150 GB RAM for NameNode alone.
    - Not POSIX: can't mount as a directory. Special APIs needed.
    - No random writes: can't update byte 500 of a file.
    - JVM overhead: NameNode is Java → GC pauses at scale.
```

### 4.3 GFS (Google File System) — The Original

```
DESIGNED FOR: Google's internal batch processing (original MapReduce).
STATUS:       replaced by Colossus internally. But foundational design.

  GFS (2003 paper) inspired HDFS. Same basic architecture:
    Master (1) = NameNode.   Stores metadata.
    Chunkservers (1000s) = DataNodes.  64 MB chunks, 3× replicated.

  Key differences from HDFS:
    - Append-optimized: primary use case was appending log data.
    - Record-level appends: multiple clients can append concurrently
      (GFS guarantees at-least-once atomic append).
    - Relaxed consistency: different clients might see different
      data for the same file region after concurrent writes.
    - Single master: became the scaling bottleneck → led to Colossus.

  Colossus (GFS2, ~2010):
    - Distributed metadata (sharded across many servers).
    - Uses Reed-Solomon erasure coding instead of 3× replication.
    - Backs BigTable, Spanner, Gmail, YouTube, everything Google.
    - Not publicly available. No paper published.
```

### 4.4 GPFS / Spectrum Scale (IBM)

```
DESIGNED FOR: enterprise HPC, large financial/research workloads.
USED BY:      banks, national labs, some AI clusters.

  Similar to Lustre in concept:
    - Parallel data access across many servers
    - POSIX compatible (mount as a filesystem)
    - Striping across disks

  Key differences from Lustre:
    - DISTRIBUTED METADATA: no single metadata server bottleneck.
      Metadata is spread across all nodes. Any node can serve
      metadata for any file. Uses distributed locking (token-based).
    - Better small-file performance (metadata not centralized).
    - Commercial product (IBM support, but expensive).
    - Cluster-wide byte-range locking (good for databases).

  Performance: comparable to Lustre for large files.
  Main advantage: more robust for mixed workloads (small + large files).
```

### 4.5 Ceph — Software-Defined Storage

```
DESIGNED FOR: unified storage (block + object + file) on commodity hw.
USED BY:      OpenStack clouds, some K8s clusters, Red Hat customers.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Three interfaces, one storage backend:                     │
  │                                                              │
  │    CephFS    → POSIX filesystem (like Lustre)               │
  │    RBD       → block device (like EBS)                      │
  │    RGW       → object store (S3-compatible)                 │
  │                                                              │
  │  All backed by RADOS (Reliable Autonomic Distributed Object │
  │  Store):                                                     │
  │                                                              │
  │    OSDs (Object Storage Daemons) — one per disk             │
  │    Monitors — consensus cluster (Paxos) for cluster state   │
  │    NO centralized metadata server for data placement!       │
  │                                                              │
  │  CRUSH algorithm:                                           │
  │    Instead of a metadata server tracking block locations,   │
  │    Ceph uses a DETERMINISTIC HASH function:                 │
  │      object_name → CRUSH(name, cluster_map) → OSD list     │
  │    Any client can compute where data lives. No lookup.      │
  │    When cluster changes (add/remove node), CRUSH             │
  │    redistributes only the affected data.                     │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Key properties:
    - No single metadata bottleneck (CRUSH computes placement)
    - Self-healing: if an OSD dies, Ceph automatically re-replicates
    - Runs on commodity hardware (no special HW needed)
    - Strong consistency (all replicas written before ACK)

  Weaknesses:
    - Performance: slower than Lustre for large sequential I/O
      (more software overhead, not optimized for InfiniBand RDMA)
    - Complexity: harder to tune for peak throughput
    - CephFS: less mature than Lustre for HPC workloads
```

### 4.6 WekaFS — Flash-Optimized Parallel FS

```
DESIGNED FOR: AI/ML workloads on all-flash clusters.
USED BY:      growing in AI training clusters (alternative to Lustre).

  Key differentiators:
    - NVMe-native: designed from scratch for flash, not spinning disks
    - POSIX compatible (mount as regular filesystem)
    - Distributed metadata (no single MDS bottleneck)
    - GPU Direct Storage: data flows NVMe → GPU, bypassing CPU
    - Tiering: hot data on NVMe, warm on SSD, cold on S3

  Why AI teams like it:
    - Better small-file performance than Lustre (ML has many small files:
      configs, tokenizers, scripts alongside large model files)
    - Lower latency than Lustre for random reads
    - Simpler operations than Lustre

  Weakness: commercial product, expensive licensing.
```

### 4.7 3FS (Fire-Flyer File System) — DeepSeek's Open-Source FS

```
DESIGNED FOR: AI training and inference on RDMA clusters.
BUILT BY:     DeepSeek (2024-2025, open-sourced Feb 2025).
USED BY:      DeepSeek internally for V3/R1 training.

  3FS was built because existing filesystems (Lustre, GPFS)
  weren't optimized for DeepSeek's specific workload:
    - AI training: huge sequential reads + checkpoint writes
    - AI inference: random reads for KV cache / model shards
    - Hardware: RDMA networking + NVMe SSDs everywhere

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Architecture:                                              │
  │                                                              │
  │  Metadata cluster (3 nodes, Raft consensus)                 │
  │    - File/directory metadata                                │
  │    - Strong consistency via Raft                            │
  │    - Handles: create, open, stat, readdir                  │
  │                                                              │
  │  Storage nodes (100s of nodes)                              │
  │    - Each manages local NVMe SSDs                          │
  │    - Data stored in chunks (configurable size)              │
  │    - RDMA for data transfers (no CPU involvement)          │
  │    - Chain replication for writes (strong consistency)      │
  │                                                              │
  │  FUSE client (on every compute node)                       │
  │    - Mounts as regular filesystem (POSIX)                  │
  │    - Userspace client (FUSE, not kernel module)            │
  │    - RDMA reads directly from storage nodes                │
  │    - Client-side striping and parallel reads               │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Key design decisions:

    1. CHAIN REPLICATION (not Raft per chunk)
       Write path: Client → Node A → Node B → Node C → ACK
       Read path: always from tail (Node C) — guaranteed latest.
       Simpler than Raft for data replication.
       Strong consistency without voting overhead per write.

    2. CRAQ (Chain Replication with Apportioned Queries)
       Reads can go to ANY node in the chain (not just tail).
       If the node has the latest version → respond immediately.
       If stale → forward to tail to confirm.
       This spreads read load across all replicas.

    3. RDMA-NATIVE from the ground up
       All data path operations use RDMA (one-sided reads).
       Client can read from storage node's memory WITHOUT the
       storage node's CPU being involved.
       Result: very low latency, very high throughput.

    4. FLAT DATA CHUNK DESIGN
       No complex striping schemes like Lustre.
       Files are split into fixed-size chunks.
       Metadata server maps: (file, offset) → (chain, chunk_id).
       Simple, predictable, easy to reason about.

  Performance (from DeepSeek's paper/repo):
    Read throughput:   ~6.2 TB/s aggregate across 180 storage nodes
    Write throughput:  ~3 TB/s aggregate
    Single client:     saturates RDMA bandwidth (~100 Gbps)
    Latency:           sub-millisecond for small reads

  Why DeepSeek built it instead of using Lustre:
    - Lustre MDS is a bottleneck for their scale of metadata ops
    - Lustre's kernel client is harder to customize than FUSE
    - They wanted RDMA-native data path (Lustre's RDMA support
      exists but isn't as tightly integrated)
    - Chain replication gives simpler consistency model
    - Open-source: full control, no vendor dependency

  Open-source: github.com/deepseek-ai/3FS (Apache 2.0)
```

---

## 5. Comparison Table

```
┌──────────────┬──────────┬──────────┬────────────┬──────────┬──────────┬──────────┐
│              │ Lustre   │ HDFS     │ GPFS       │ Ceph     │ WekaFS   │ 3FS      │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Interface    │ POSIX    │ Custom   │ POSIX      │ POSIX/   │ POSIX    │ POSIX    │
│              │          │ Java API │            │ S3/Block │          │ (FUSE)   │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Metadata     │ Central  │ Central  │ Distributed│ CRUSH    │ Distrib. │ Raft     │
│              │ (MDS)    │(NameNode)│ (all nodes)│ (no MDS) │          │ (3-node) │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Replication  │ RAID /   │ 3× rep   │ Rep / EC   │ 3× rep   │ EC       │ Chain    │
│              │ rep      │ or EC    │            │ or EC    │          │ rep (3×) │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Large file   │ Excellent│ Good     │ Excellent  │ Good     │ V. good  │ Excellent│
│ throughput   │          │          │            │          │          │          │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Small files  │ Poor     │ Poor     │ Better     │ OK       │ Good     │ OK       │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Random R/W   │ OK       │ None     │ Good       │ OK       │ Good     │ Good     │
│              │          │(append)  │            │          │          │ (RDMA)   │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Network      │ IB/RDMA  │ TCP      │ IB/RDMA   │ TCP/RDMA │ IB/RDMA  │ RDMA     │
│              │          │          │            │          │          │ native   │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Scale        │ PB-EB    │ PB-EB    │ PB-EB     │ PB-EB    │ PB       │ PB       │
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ Best for     │ HPC, ML  │ Hadoop   │ Enterprise │ Cloud    │ ML (flash│ ML (RDMA │
│              │ training │ analytics│ HPC        │ storage  │ clusters)│ clusters)│
├──────────────┼──────────┼──────────┼────────────┼──────────┼──────────┼──────────┤
│ License      │ Open src │ Open src │ Commercial │ Open src │ Commerc. │ Open src │
└──────────────┴──────────┴──────────┴────────────┴──────────┴──────────┴──────────┘
```

---

## 6. Object Storage vs Filesystem — Why Both Exist

```
Object stores (S3, GCS, Azure Blob) are NOT filesystems:

  ┌─────────────────────┬──────────────────────────────────────┐
  │ Filesystem (POSIX)  │ Object Store (S3-like)               │
  ├─────────────────────┼──────────────────────────────────────┤
  │ Hierarchical dirs   │ Flat key-value namespace             │
  │ open/read/write/    │ PUT/GET/DELETE by key                │
  │ seek/close          │                                      │
  │ Random byte access  │ Read/write whole objects only        │
  │ Mutable files       │ Immutable (replace whole object)     │
  │ Locks, permissions  │ IAM policies, ACLs                   │
  │ Low latency (<1ms)  │ Higher latency (~50-100ms)           │
  │ Limited scale       │ Unlimited scale                      │
  │ Complex to operate  │ Fully managed (cloud)                │
  └─────────────────────┴──────────────────────────────────────┘

  ML training uses BOTH:
    Lustre/GPFS: hot path — training data reads, checkpoint writes.
      Needs: low latency, high throughput, POSIX (torch.save just works).

    S3/GCS: cold storage — raw datasets, final model artifacts, backups.
      Needs: cheap, unlimited capacity, no ops overhead.

    Typical flow:
      Raw data on S3 → copy to Lustre → train → checkpoint to Lustre
      → final model to S3 → serve from S3 or model registry.
```

---

## 7. Key Concepts

```
STRIPING:
  Split a file into chunks, spread across multiple servers.
  More servers reading simultaneously = more bandwidth.
  Lustre, GPFS, Ceph all do this.

REPLICATION:
  Store 3 copies of each chunk on different servers.
  Simple, fast reads (read from any copy), but 3× storage cost.
  HDFS default, Ceph default.

ERASURE CODING:
  Like RAID 6 across servers. Store N data + M parity chunks.
  Can lose M servers and recover. Only ~1.5× storage cost.
  Used by: S3, newer HDFS, Ceph, Colossus.

DATA LOCALITY:
  Move compute to where the data is, not data to compute.
  Hadoop's key insight: schedule MapReduce task on the DataNode
  that holds the data → zero network transfer.
  Less relevant for GPU training (data must go to GPU regardless).

RACK AWARENESS:
  Place replicas on different racks so a rack power failure
  doesn't lose all copies. HDFS, Ceph, Lustre all support this.

METADATA SCALABILITY:
  The #1 bottleneck in most distributed filesystems.
  Single metadata server: simple but limits file count.
  Solutions: shard metadata (GPFS, Ceph CRUSH, Colossus).

CACHE COHERENCE:
  When multiple clients modify the same file simultaneously:
    Lustre: byte-range locking (client acquires lock before write)
    GPFS: token-based distributed locking
    HDFS: doesn't support concurrent writes (append-only)
    Ceph: MDS-coordinated locks for CephFS
```
