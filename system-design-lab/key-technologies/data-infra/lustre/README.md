# Lustre — The HPC Parallel Filesystem

---

## 1. What Lustre Is and Why It Exists

```
Lustre is a POSIX-compliant parallel distributed filesystem designed
for maximum throughput on large sequential I/O. Open-source (GPLv2).

  Origins:
    - Started at Carnegie Mellon University (1999, Peter Braam)
    - Name: "Linux" + "Cluster"
    - Funded by US Department of Energy for supercomputers
    - Now maintained by DDN (DataDirect Networks) + open community
    - Powers 60%+ of the world's top supercomputers

  Used by:
    - National labs: ORNL (Frontier), LLNL (El Capitan), ANL (Aurora)
    - AI companies: Meta (LLaMA training), NVIDIA
    - Cloud HPC: AWS FSx for Lustre, Azure Managed Lustre
    - Any site needing 100s of GB/s aggregate throughput

  Core design goal:
    Make 1000s of disks across 100s of servers look like a single
    POSIX filesystem. Any client can read/write any file.
    Aggregate bandwidth scales linearly with server count.

  What Lustre is NOT:
    - Not an object store (it's POSIX: open/read/write/close)
    - Not optimized for small files (designed for large I/O)
    - Not a cloud-native system (designed for bare-metal HPC clusters)
```

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         LUSTRE CLUSTER                               │
│                                                                      │
│  MGS (Management Server) — 1 server                                 │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │  Stores cluster configuration (which MDS, which OSS, etc.)    │ │
│  │  Clients contact MGS first on mount to learn cluster topology.│ │
│  │  Not in the data path. Contacted only on mount or config      │ │
│  │  change. Can run on the same node as MDS.                     │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  MDS (Metadata Servers) — 1-4 servers (active-active with DNE)      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                                                                │ │
│  │  MDS 0 ──── MDT 0                   MDS 1 ──── MDT 1         │ │
│  │  (primary)  (metadata target)        (DNE)  (metadata target) │ │
│  │                                                                │ │
│  │  Each MDT is a local filesystem (ldiskfs or ZFS) on SSD.     │ │
│  │                                                                │ │
│  │  Stores:                                                       │ │
│  │    - Inodes (file metadata: size, permissions, timestamps)    │ │
│  │    - Directory entries (dentries: name → inode mapping)        │ │
│  │    - File layout (which OSTs hold which stripes)              │ │
│  │    - Extended attributes (xattrs)                              │ │
│  │    - FID → inode mapping (FID = Lustre File IDentifier)       │ │
│  │                                                                │ │
│  │  NOT stored on MDS: actual file data (that's on OSTs)         │ │
│  │                                                                │ │
│  │  Backed by: ldiskfs (ext4 fork, faster) or ZFS (checksums)   │ │
│  │  Runs on: fast SSDs, lots of RAM for inode cache              │ │
│  │                                                                │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  OSSes + OSTs (Object Storage Servers + Targets) — 10s to 100s     │
│  ┌──────────────┐  ┌──────────────┐      ┌──────────────┐         │
│  │ OSS 0        │  │ OSS 1        │      │ OSS N        │         │
│  │              │  │              │      │              │         │
│  │ OST 0  OST 1│  │ OST 2  OST 3│      │ OST 2N OST.. │         │
│  │ [disks]      │  │ [disks]      │      │ [disks]      │         │
│  │              │  │              │      │              │         │
│  │ Each OST =   │  │ Each OST =   │      │ Each OST =   │         │
│  │ local fs     │  │ local fs     │      │ local fs     │         │
│  │ (ldiskfs/ZFS)│  │ (ldiskfs/ZFS)│      │ (ldiskfs/ZFS)│         │
│  │ on a RAID    │  │ on a RAID    │      │ on a RAID    │         │
│  │ array or JBOD│  │ array or JBOD│      │ array or JBOD│         │
│  │              │  │              │      │              │         │
│  │ InfiniBand   │  │ InfiniBand   │      │ InfiniBand   │         │
│  │ NIC          │  │ NIC          │      │ NIC          │         │
│  └──────────────┘  └──────────────┘      └──────────────┘         │
│                                                                      │
│  CLIENTS — Linux kernel module on every compute/GPU node            │
│  ┌──────────────┐  ┌──────────────┐      ┌──────────────┐         │
│  │ Client 0     │  │ Client 1     │      │ Client M     │         │
│  │              │  │              │      │              │         │
│  │ mount -t     │  │ mount -t     │      │ mount -t     │         │
│  │ lustre       │  │ lustre       │      │ lustre       │         │
│  │ /mnt/lustre  │  │ /mnt/lustre  │      │ /mnt/lustre  │         │
│  │              │  │              │      │              │         │
│  │ Kernel module│  │ Kernel module│      │ Kernel module│         │
│  │ (llite, lov, │  │              │      │              │         │
│  │  osc, mdc,   │  │ torch.save() │      │ 8× GPUs      │         │
│  │  lnet)       │  │ just works   │      │              │         │
│  └──────────────┘  └──────────────┘      └──────────────┘         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘

Terminology:
  MGS  = Management Server (cluster config, contacted on mount)
  MDS  = Metadata Server (serves metadata from MDTs)
  MDT  = Metadata Target (the actual disk/fs holding metadata)
  OSS  = Object Storage Server (serves data from OSTs)
  OST  = Object Storage Target (the actual disk/fs holding file data)
  LNet = Lustre Networking (abstraction over InfiniBand, TCP, etc.)
  FID  = File Identifier (128-bit unique ID for every file/dir)
```

---

## 3. The Client Kernel Module — How Apps Talk to Lustre

```
Lustre's client is a LINUX KERNEL MODULE, not a FUSE driver.
This is a critical design choice.

  $ mount -t lustre mgs@o2ib:/testfs /mnt/lustre

  After mounting, /mnt/lustre looks like a normal directory.
  ls, cat, cp, dd, torch.save(), fopen() — all POSIX calls work.
  The application has NO idea it's talking to a distributed filesystem.

  Inside the kernel module, there are several layers:

  ┌──────────────────────────────────────────────────────────────┐
  │  Application                                                 │
  │    │                                                         │
  │    │  POSIX syscall: read(fd, buf, 4MB)                     │
  │    ▼                                                         │
  │  VFS (Linux Virtual Filesystem Switch)                      │
  │    │                                                         │
  │    │  VFS dispatches to the Lustre filesystem driver         │
  │    ▼                                                         │
  │  llite (Lustre Lite — the VFS interface)                    │
  │    │  Translates VFS ops → Lustre internal ops              │
  │    │                                                         │
  │    ├──► mdc (Metadata Client)                               │
  │    │      Talks to MDS for: open, stat, readdir, unlink     │
  │    │      Caches metadata locally (inode cache, dentry cache)│
  │    │                                                         │
  │    ├──► lov (Logical Object Volume)                         │
  │    │      Handles STRIPING logic.                            │
  │    │      Knows: "this file's bytes 0-1MB are on OST 3,    │
  │    │              bytes 1-2MB are on OST 7, etc."           │
  │    │      Splits/merges I/O across stripe targets.          │
  │    │                                                         │
  │    └──► osc (Object Storage Client) — one per OST          │
  │           Sends read/write RPCs to the corresponding OSS.   │
  │           Handles RPC batching, flow control, checksums.     │
  │           Each osc manages a connection to one OST.          │
  │                                                              │
  │  All network I/O goes through LNet:                         │
  │  ┌──────────────────────────────────────────────────────┐   │
  │  │  LNet (Lustre Networking)                            │   │
  │  │    Abstracts transport: o2ib (IB verbs), tcp, etc.   │   │
  │  │    Handles routing between networks.                  │   │
  │  │    RDMA transfers for bulk data (zero-copy).         │   │
  │  └──────────────────────────────────────────────────────┘   │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Why kernel module instead of FUSE:
    + No context switches for each I/O (FUSE goes kernel→user→kernel)
    + Direct page cache integration (Lustre pages = Linux page cache)
    + RDMA zero-copy: data goes NIC → kernel buffer → app (no extra copy)
    + Sub-microsecond VFS path (no FUSE daemon overhead)

    - Harder to develop and debug (kernel crashes = machine crashes)
    - Must match kernel version (Lustre client tied to specific kernels)
    - Harder to deploy (need to build/install kernel module)
```

---

## 4. File Striping — How Data Is Spread Across OSTs

```
Striping is Lustre's core mechanism for parallel I/O.

  A file is divided into fixed-size STRIPES, round-robined across OSTs.

  $ lfs setstripe -c 4 -S 1M /mnt/lustre/myfile.bin
    -c 4   = stripe count: use 4 OSTs
    -S 1M  = stripe size: each stripe is 1 MB

  Writing a 10 MB file with stripe_count=4, stripe_size=1M:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  File offset:  [0  1  2  3  4  5  6  7  8  9] MB           │
  │                                                              │
  │  OST 12:       [0]          [4]          [8]                │
  │  OST 37:          [1]          [5]          [9]             │
  │  OST 55:             [2]          [6]                       │
  │  OST 81:                [3]          [7]                    │
  │                                                              │
  │  Round-robin: byte 0→OST12, byte 1M→OST37, byte 2M→OST55, │
  │               byte 3M→OST81, byte 4M→OST12 (wraps), ...    │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  When reading this file:
    lov layer splits read(0, 10MB) into 4 parallel sub-reads:
      osc for OST 12: read stripes at offsets 0, 4, 8
      osc for OST 37: read stripes at offsets 1, 5, 9
      osc for OST 55: read stripes at offsets 2, 6
      osc for OST 81: read stripes at offsets 3, 7
    All 4 RPCs sent in PARALLEL over LNet.
    lov reassembles the stripes into the correct order.
    App sees a contiguous 10 MB buffer.

  Stripe count tradeoffs:
    stripe_count=1 (default):
      - Good for small files (no coordination overhead)
      - Single OST throughput only (~2-5 GB/s)

    stripe_count=4:
      - Good for medium files (checkpoints, model weights)
      - 4× single-OST throughput

    stripe_count=-1 (stripe across ALL OSTs):
      - Maximum throughput (every OST contributes)
      - Good for: huge checkpoint files
      - Bad for: wastes OST resources for small files
      - Used by: Meta for LLaMA checkpoints

  Stripe size tradeoffs:
    Small (64K-256K):
      - More parallelism for small reads
      - More RPCs, higher overhead
    Large (1M-4M):
      - Fewer RPCs, lower overhead
      - Less parallelism for reads smaller than stripe_size
      - Default: 1 MB (good balance)

  ┌──────────────────────────────────────────────────────────────┐
  │  Example: 100 GB checkpoint, 200 OSTs, stripe_count=-1     │
  │                                                              │
  │  Each OST holds: 100 GB / 200 = 500 MB                     │
  │  Each OST @ 5 GB/s → per-OST read: 0.1 seconds            │
  │  ALL OSTs read in parallel → 100 GB in 0.1 seconds         │
  │  Aggregate: 200 × 5 GB/s = 1 TB/s                          │
  │                                                              │
  │  vs single OST: 100 GB / 5 GB/s = 20 seconds               │
  │  Striping speedup: 200×                                     │
  └──────────────────────────────────────────────────────────────┘

  Progressive File Layout (PFL) — Lustre 2.10+:
    Different stripe settings for different regions of a file.

    $ lfs setstripe -E 1M -c 1 -E 1G -c 4 -E -1 -c -1 myfile
      Bytes 0-1M:    stripe_count=1 (small, single OST)
      Bytes 1M-1G:   stripe_count=4 (medium, 4 OSTs)
      Bytes 1G+:     stripe_count=-1 (large, all OSTs)

    Starts narrow, widens as file grows. Smart and automatic.
```

---

## 5. LNet — Lustre Networking

```
LNet is Lustre's network abstraction layer.
All communication (metadata + data) flows through LNet.

  Supported transports:
    o2ib   — InfiniBand verbs (native RDMA, the fast path)
    tcp    — TCP/IP sockets (fallback, much slower)
    gni    — Cray Aries interconnect (Cray supercomputers)
    kfi    — Kernel fabric interface (newer)

  LNet key concepts:

  1. NID (Network IDentifier):
     Format: IP@network_type  or  address@network_type
     Example: 10.0.1.42@o2ib   (InfiniBand)
              10.0.1.42@tcp    (TCP)

  2. ROUTING:
     LNet can route between different network types.
     GPU nodes on InfiniBand → LNet router → storage on Ethernet
     (though this is rare; most HPC sites use IB everywhere)

  3. RDMA BULK TRANSFER:
     For data I/O (not metadata), LNet uses RDMA:

     ┌────────────────────────────────────────────────────────┐
     │                                                        │
     │  Client sends RPC to OSS:                             │
     │    "I want to read 1 MB at offset X from OST 5"      │
     │    RPC includes: RDMA descriptor (client buffer addr)  │
     │                                                        │
     │  OSS reads data from disk into its buffer.            │
     │  OSS performs RDMA WRITE into client's memory buffer. │
     │  Data bypasses client CPU — arrives directly in RAM.  │
     │                                                        │
     │  For writes: reverse direction.                        │
     │  Client RDMA PUTs data into OSS buffer.               │
     │  OSS writes to disk.                                  │
     │                                                        │
     │  "Bulk I/O" = RDMA. Only control messages use send/recv│
     │                                                        │
     └────────────────────────────────────────────────────────┘

  4. MULTI-RAIL:
     A single node can have multiple InfiniBand ports.
     LNet stripes traffic across them for higher bandwidth.
     Example: 2× HDR InfiniBand = 2×200 Gbps = 400 Gbps per client.

  Performance over InfiniBand:
    Per-client: 10-25 GB/s (depends on stripe count and IB generation)
    HDR InfiniBand: 200 Gbps = 25 GB/s theoretical per port
    NDR InfiniBand: 400 Gbps = 50 GB/s theoretical per port
    Lustre overhead: ~10-20% below wire speed (RPCs, protocol, etc.)
```

---

## 6. Locking — How Concurrent Access Works

```
Multiple clients can read AND write the same file simultaneously.
Lustre uses DISTRIBUTED LOCKS to maintain POSIX consistency.

  The lock manager is called LDLM (Lustre Distributed Lock Manager).
  Locks are managed by the MDS (for metadata) and OSSes (for data).

  Lock types:
    - CR (Concurrent Read): multiple readers, no writers
    - CW (Concurrent Write): multiple writers (each to different regions)
    - PR (Protected Read): exclusive read (for read-your-own-writes)
    - PW (Protected Write): exclusive write to a byte range
    - EX (Exclusive): full exclusive access

  How byte-range locking works:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Client A: write(fd, buf, 1MB) at offset 0                 │
  │    1. osc requests PW lock on [0, 1MB) from OST             │
  │    2. OST grants PW lock (no conflict)                      │
  │    3. Client A writes 1 MB                                  │
  │                                                              │
  │  Client B: write(fd, buf, 1MB) at offset 4MB               │
  │    1. osc requests PW lock on [4MB, 5MB) from OST           │
  │    2. OST grants PW lock (no conflict — different range)    │
  │    3. Client B writes 1 MB in parallel with Client A        │
  │                                                              │
  │  Client C: write(fd, buf, 1MB) at offset 0                 │
  │    1. osc requests PW lock on [0, 1MB) from OST             │
  │    2. CONFLICT with Client A's lock                         │
  │    3. OST sends AST (Asynchronous Blocking Callback) to A  │
  │    4. Client A flushes dirty data, releases lock            │
  │    5. OST grants lock to Client C                           │
  │    6. Client C writes its 1 MB                              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Lock CALLBACKS (the key mechanism):

    When a lock conflicts, the server doesn't just deny.
    It sends a BLOCKING AST (callback) to the current holder:
      "Someone else wants this range. Flush your data and release."

    This is why Lustre can maintain POSIX semantics across 1000s
    of clients without each client checking locks continuously.

  Metadata locks (managed by MDS):

    When you open a file, the MDS grants a metadata lock.
    If another client modifies the file (e.g., chmod, rename),
    the MDS revokes conflicting metadata locks via callbacks.

    Common bottleneck: single directory with thousands of creates:
      Each create needs an exclusive lock on the parent directory.
      Serialized. This is why Lustre is bad at "many small files."

  Lock CANCELLATION for ML workloads:

    ML training typically has this pattern:
      N workers read the same large files (training data).
      → All get CR locks. No conflict. Fast.

    Checkpoint writing:
      Each worker writes a DIFFERENT file (rank-specific checkpoint).
      → No lock conflicts at all. Fast.

    Worst case for Lustre:
      Many processes creating files in the same directory.
      Or many processes appending to the same file.
      → Heavy lock contention. Slow.
```

---

## 7. Metadata Operations — The MDS Deep Dive

```
The MDS is Lustre's most critical (and historically most fragile)
component.

  What the MDS does:
    open()    → look up inode, return file layout (stripe info)
    stat()    → return inode attributes (size, mtime, permissions)
    readdir() → list directory entries
    create()  → allocate inode + assign OSTs for striping
    unlink()  → remove file, free space on OSTs
    rename()  → atomically move a dentry

  How MDS stores metadata:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  MDT (Metadata Target) — a local filesystem on SSD          │
  │                                                              │
  │  ldiskfs (default):                                         │
  │    Fork of ext4 optimized for Lustre.                       │
  │    Changes: larger inodes (for storing stripe layout in     │
  │    inode xattrs), journal tweaks, bulk orphan cleanup.      │
  │    Performance: ~100K-300K metadata ops/sec per MDS.        │
  │                                                              │
  │  ZFS (alternative):                                         │
  │    Checksums every block (detects silent corruption).       │
  │    Snapshots (cheap metadata snapshots).                    │
  │    Slower than ldiskfs (~60-70% of ldiskfs performance).    │
  │    Chosen when data integrity is paramount.                 │
  │                                                              │
  │  Layout stored in inode xattr:                              │
  │    struct lov_mds_md {                                      │
  │      stripe_count: 4,                                       │
  │      stripe_size: 1048576,  // 1 MB                        │
  │      objects: [                                             │
  │        { ost_idx: 12, object_id: 0x1a3f },                 │
  │        { ost_idx: 37, object_id: 0x2b4e },                 │
  │        { ost_idx: 55, object_id: 0x3c5d },                 │
  │        { ost_idx: 81, object_id: 0x4d6c },                 │
  │      ]                                                      │
  │    }                                                        │
  │    Stored directly in the inode extended attribute.          │
  │    No separate lookup needed. open() returns this layout.   │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  DNE (Distributed Namespace) — Lustre 2.4+:

    Problem: single MDS limits metadata throughput.
    Solution: shard the directory tree across multiple MDTs.

    Two modes:

    1. Remote directories (DNE1):
       /mnt/lustre/user_A/  → MDT 0
       /mnt/lustre/user_B/  → MDT 1
       Manually assigned. Different subtrees on different MDTs.

    2. Striped directories (DNE2, Lustre 2.8+):
       A single directory's entries are hashed across multiple MDTs.
       /mnt/lustre/checkpoints/ striped across MDT 0, 1, 2, 3.
       create("file_rank_42") → hash("file_rank_42") → MDT 2.
       Parallel creates in the same directory!

    ┌────────────────────────────────────────────────────────┐
    │  /checkpoints/ striped across 4 MDTs:                 │
    │                                                        │
    │  MDT 0: file_rank_0, file_rank_4, file_rank_8, ...   │
    │  MDT 1: file_rank_1, file_rank_5, file_rank_9, ...   │
    │  MDT 2: file_rank_2, file_rank_6, file_rank_10, ...  │
    │  MDT 3: file_rank_3, file_rank_7, file_rank_11, ...  │
    │                                                        │
    │  4 MDTs handle creates in parallel → 4× metadata speed │
    └────────────────────────────────────────────────────────┘

  MDS performance (single MDS, ldiskfs):
    stat():     ~300K ops/sec
    create():   ~50K-100K ops/sec
    readdir():  depends on directory size
    Bottleneck: journal commit (fsync per metadata transaction)
```

---

## 8. Read and Write Paths — Step by Step

```
READ PATH — reading 4 MB from a striped file:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  1. App: read(fd, buf, 4MB) at offset 0                    │
  │     ↓                                                        │
  │  2. VFS → llite: already has file layout from open()        │
  │     Layout: stripe_count=4, stripe_size=1M,                 │
  │             OSTs = [12, 37, 55, 81]                         │
  │     ↓                                                        │
  │  3. lov splits the read:                                    │
  │       osc_12: read 1 MB at offset 0                         │
  │       osc_37: read 1 MB at offset 0                         │
  │       osc_55: read 1 MB at offset 0                         │
  │       osc_81: read 1 MB at offset 0                         │
  │     ↓                                                        │
  │  4. Each osc checks: do I have a lock on this range?        │
  │     If cached data + valid lock → serve from page cache.    │
  │     If no lock/data → send RPC to OSS.                      │
  │     ↓                                                        │
  │  5. Each osc sends bulk read RPC to its OSS over LNet:     │
  │     RPC includes: object_id, offset, length, RDMA desc     │
  │     ↓                                                        │
  │  6. Each OSS reads from local disk (ldiskfs/ZFS on the OST)│
  │     OSS RDMA-writes data directly into client's memory.     │
  │     ↓                                                        │
  │  7. All 4 RPCs complete. lov merges pages into correct order│
  │     ↓                                                        │
  │  8. VFS copies data to user buffer. read() returns 4 MB.   │
  │                                                              │
  │  Total latency: ~1 network RTT + disk read time             │
  │  (all 4 OSTs read in parallel, so latency ≈ single-OST)    │
  │  Throughput: 4 × single-OST bandwidth                       │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘


WRITE PATH — writing 4 MB to a striped file:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  1. App: write(fd, buf, 4MB) at offset 0                   │
  │     ↓                                                        │
  │  2. VFS → llite → lov splits into 4 stripes (same as read) │
  │     ↓                                                        │
  │  3. Each osc acquires PW lock on its byte range from OST.  │
  │     (If already held from previous write → skip)            │
  │     ↓                                                        │
  │  4. Data goes into CLIENT PAGE CACHE (dirty pages).         │
  │     write() returns immediately (async by default).          │
  │                                                              │
  │     The data is NOT yet on disk on the OST.                 │
  │     It's in the client's page cache, marked dirty.          │
  │     ↓                                                        │
  │  5. Background writeback flushes dirty pages to OSTs:       │
  │     Each osc collects dirty pages for its OST.             │
  │     Sends bulk write RPC (client RDMA read by OSS).        │
  │     ↓                                                        │
  │  6. OSS writes to local disk. ACKs the RPC.                │
  │     Client marks pages clean.                                │
  │     ↓                                                        │
  │  7. For fsync()/O_SYNC: write waits for OSS ACK before     │
  │     returning. Data on disk. Durable.                        │
  │                                                              │
  │  IMPORTANT: default write is ASYNC.                         │
  │    Data is in client cache. If client crashes before flush,  │
  │    data is LOST. This is the same as local ext4 behavior.   │
  │    For checkpoints: always fsync() or O_SYNC.               │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘


WRITE REPLICATION — NOT like 3FS

  Lustre does NOT replicate data across OSTs by default.
  Each stripe lives on exactly ONE OST.
  Durability comes from the OST's local RAID array.

  ┌────────────────────────────────────────────────────────────┐
  │                                                            │
  │  3FS:    Client → Head → Middle → Tail (3× replication)  │
  │  Lustre: Client → OST (single copy on RAID array)        │
  │                                                            │
  │  If an OST dies:                                          │
  │    3FS: data still available on other chain nodes          │
  │    Lustre: data is LOST unless the OST's RAID recovers    │
  │                                                            │
  │  Lustre's assumption:                                      │
  │    Each OST is a RAID-6 array (can lose 2 disks).         │
  │    Server has redundant power, ECC RAM, etc.              │
  │    The RAID protects against disk failure.                 │
  │    But if the entire OSS/node dies → data unavailable.    │
  │                                                            │
  │  Mitigations:                                              │
  │    - OST mirroring (Lustre 2.11+): file-level mirroring   │
  │      across 2-3 OSTs (like RAID-1 at file level)          │
  │    - Backup to object store (Lustre HSM)                  │
  │    - Application-level: checkpoint to 2+ Lustre clusters  │
  │                                                            │
  └────────────────────────────────────────────────────────────┘
```

---

## 9. The Page Cache and Read-Ahead

```
Lustre leverages the Linux page cache heavily.

  Read-ahead:
    When client reads sequentially, Lustre detects the pattern.
    Client pre-fetches upcoming stripes before the app asks.
    Read-ahead window grows as sequential pattern continues.

    ┌────────────────────────────────────────────────────────┐
    │  App reads: [0-1MB]                                    │
    │  Lustre pre-fetches: [1-5MB] (read-ahead window = 4MB)│
    │                                                        │
    │  App reads: [1-2MB]  → served from cache! Zero latency│
    │  Lustre pre-fetches: [5-13MB] (window grows to 8MB)   │
    │                                                        │
    │  App reads: [2-3MB]  → served from cache!              │
    │  Lustre pre-fetches: [13-29MB] (window grows to 16MB) │
    │                                                        │
    │  Max read-ahead window: configurable (default ~40 MB)  │
    │  Per-file. Per-client. Scales with stripe count.       │
    └────────────────────────────────────────────────────────┘

  Write-behind:
    Writes go to page cache (dirty pages).
    Background threads flush to OSTs when:
      - Dirty ratio exceeds threshold
      - Periodic timer fires
      - fsync() is called
      - Lock is revoked (another client wants the range)

  Page cache coherence:
    Lustre uses LOCKS to maintain cache coherence.
    When Client A holds a read lock + cached data, and Client B
    writes the same range:
      1. OST sends blocking AST to Client A
      2. Client A invalidates cached pages for that range
      3. Client A releases its lock
      4. Client B's write proceeds
      5. Next time Client A reads → cache miss → fresh data from OST

    This is expensive but guarantees POSIX semantics.
    In practice, ML workloads rarely have true file-level conflicts.
```

---

## 10. Failure Handling

```
OSS/OST failure:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  OSS 5 (hosting OST 10, OST 11) crashes.                   │
  │                                                              │
  │  Immediate effect:                                          │
  │    - Any file with stripes on OST 10 or 11 is partially    │
  │      unavailable. I/O to those stripes blocks (hangs).      │
  │    - I/O to stripes on OTHER OSTs continues fine.           │
  │    - Entire files with stripe_count=1 on OST 10/11 → stuck.│
  │                                                              │
  │  Recovery:                                                  │
  │    1. OSS comes back up.                                    │
  │    2. OSTs replay their journals (ldiskfs/ZFS journal).     │
  │    3. Clients reconnect and replay uncommitted RPCs.        │
  │    4. I/O resumes. Automatic.                               │
  │                                                              │
  │  If OSS is permanently dead:                                │
  │    - Admin must replace hardware, restore OST from backup.  │
  │    - Files with stripes on dead OST are degraded until      │
  │      OST is restored or file is re-striped.                 │
  │    - No automatic re-replication (unlike 3FS or Ceph).      │
  │                                                              │
  │  This is Lustre's biggest operational weakness.             │
  │  Hardware failure = manual intervention.                     │
  │  (Contrast: 3FS automatically re-replicates lost chunks.)  │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘


MDS failure:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  MDS crashes.                                               │
  │                                                              │
  │  Immediate effect:                                          │
  │    - No new file opens, creates, stats, readdir.            │
  │    - In-flight reads/writes that already have file layout   │
  │      and locks can CONTINUE (data path is OSS-direct).      │
  │    - But any operation needing metadata → hangs.            │
  │                                                              │
  │  Recovery:                                                  │
  │    1. MDS comes back up (or standby MDS takes over via HA). │
  │    2. MDT journal is replayed.                               │
  │    3. Clients re-establish connections, replay uncommitted   │
  │       metadata operations.                                   │
  │    4. Service resumes.                                       │
  │                                                              │
  │  HA setup (recommended):                                    │
  │    Active-passive MDS pair. Shared storage for MDT.         │
  │    If active dies → passive takes over the MDT.             │
  │    Failover time: ~30 seconds to 2 minutes (slow!).        │
  │    Clients see a pause but no data loss.                    │
  │                                                              │
  │  This is worse than 3FS's Raft:                             │
  │    3FS: ~1 second failover (Raft elects new leader).       │
  │    Lustre: 30-120 seconds (active-passive takeover).        │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘


Client failure:

  A client crash is relatively harmless to the cluster.
  - The MDS and OSSes detect the dead client (timeout).
  - Locks held by the dead client are revoked.
  - Dirty pages in the dead client's cache are lost (unless fsync'd).
  - Other clients are unaffected.
```

---

## 11. Lustre HSM (Hierarchical Storage Management)

```
HSM lets Lustre tier data to cheaper storage (tape, S3, etc.).

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Hot tier:  Lustre OSTs (NVMe/SSD, fast, expensive)        │
  │  Cold tier: Tape / S3 / cheaper disks (slow, cheap)        │
  │                                                              │
  │  Operations:                                                │
  │    ARCHIVE: copy file from Lustre to cold tier              │
  │    RELEASE: free OST space (file appears as stub on MDS)    │
  │    RESTORE: copy file back from cold tier to Lustre         │
  │                                                              │
  │  When a released file is read:                              │
  │    1. Client open() → MDS sees file is "released"           │
  │    2. MDS triggers RESTORE from archive                     │
  │    3. Client blocks until file is restored                   │
  │    4. read() proceeds normally                               │
  │                                                              │
  │  Use case:                                                  │
  │    Training data that's only needed for specific runs.      │
  │    Archive after training. Release space.                    │
  │    Re-stage before next training run.                        │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  AWS FSx for Lustre uses this:
    S3 bucket as the archive tier.
    Data lazily loads from S3 into Lustre on first read.
    Training completes → data evicted → S3 remains.
```

---

## 12. Use Cases in ML Training

```
1. TRAINING DATA STORAGE
   Pre-tokenized data shards stored as large binary files.
   Each data-parallel worker reads a different shard (or offset).
   Stripe across many OSTs for maximum read throughput.
   Sequential reads → Lustre read-ahead maximizes bandwidth.

   $ lfs setstripe -c -1 -S 4M /mnt/lustre/training_data/
   # Every file striped across all OSTs, 4 MB stripe size.

2. CHECKPOINT WRITES
   Every N steps: each rank writes its model shard.
   Write to rank-specific files → no lock contention.
   Critical: use fsync() to ensure durability.

   $ lfs setstripe -c 8 -S 1M /mnt/lustre/checkpoints/
   # 8 OSTs per file. All ranks write in parallel.
   # 1000 ranks × 8 OSTs each = saturate the cluster.

3. MODEL WEIGHTS (load at startup)
   Model weights stored as a few large files.
   All ranks read the same files simultaneously.
   Lustre's page cache on OSSes helps (read once from disk,
   serve many clients from memory).

4. SHARED CONFIG / SMALL FILES
   tokenizer.json, config.yaml, scripts.
   Lustre is BAD at this. Metadata ops are slow.
   Workaround: copy small files to local /tmp at job start.
   Or use a single tarball that each node extracts locally.

Typical ML cluster Lustre config:
  - 50-200 OSSes, each with 2-4 NVMe OSTs
  - 1-2 MDSes (with DNE for checkpoint directories)
  - InfiniBand HDR/NDR
  - Per-client throughput: 10-25 GB/s
  - Aggregate: 500 GB/s - 2 TB/s
```

---

## 13. Operational Pain Points

```
Lustre is powerful but operationally demanding:

  1. KERNEL MODULE DEPENDENCY
     Lustre client must match the kernel version.
     Kernel upgrade → must rebuild Lustre client.
     DKMS helps but doesn't always work.
     Containers: must mount Lustre on the host, bind-mount in.

  2. OST IMBALANCE
     Some OSTs fill up faster than others.
     New file creates pick OSTs by available space (weighted random).
     But old files don't rebalance. Manual "lfs migrate" needed.

  3. METADATA BOTTLENECK (pre-DNE)
     Single MDS: ls on a directory with 1M files = slow.
     fix: DNE striped directories. But requires planning.

  4. NO AUTOMATIC RE-REPLICATION
     OST dies → admin must fix hardware.
     No self-healing like Ceph or 3FS.
     Must maintain RAID arrays carefully.

  5. DEBUGGING IS HARD
     Kernel module crashes → node reboots.
     Lustre has its own debug log (/proc/sys/lnet/debug).
     Error messages are cryptic.
     Requires specialized Lustre admin knowledge.

  6. QUOTA MANAGEMENT
     Lustre supports user/group/project quotas.
     But quota enforcement is distributed (MDS + OSSes must agree).
     Quota can become slightly inaccurate under heavy load.
```

---

## 14. Lustre vs 3FS — Detailed Comparison

```
┌──────────────────────┬──────────────────┬──────────────────────┐
│                      │ Lustre           │ 3FS                  │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Client type          │ Kernel module    │ FUSE (userspace)     │
│                      │ (fast, fragile)  │ (slower, safe)       │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Data replication     │ None (RAID per   │ Chain replication    │
│                      │ OST) or mirroring│ 3× across nodes      │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Data placement       │ Striping across  │ Chunks assigned to   │
│                      │ OSTs (round-robin│ chains by metadata   │
│                      │ within a file)   │ cluster              │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Read distribution    │ Each stripe from │ CRAQ: any chain node │
│                      │ its single OST   │ can serve reads      │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Consistency model    │ POSIX locks      │ Chain tail commit    │
│                      │ (LDLM)           │ (simpler)            │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Metadata             │ MDS (central,    │ Raft (3-node,        │
│                      │ DNE for scaling) │ auto-failover ~1s)   │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Metadata failover    │ Active-passive   │ Raft election        │
│                      │ (30-120s)        │ (~1 second)          │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Network              │ LNet (IB, TCP,   │ RDMA-native (IB/RoCE │
│                      │ Cray, routing)   │ only, no TCP fallback│
├──────────────────────┼──────────────────┼──────────────────────┤
│ Self-healing         │ No (manual RAID/ │ Yes (automatic       │
│                      │ OST recovery)    │ re-replication)      │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Storage media        │ Any (HDD, SSD,   │ NVMe only            │
│                      │ NVMe, RAID)      │                      │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Maturity             │ 20+ years        │ New (2025)           │
│                      │ Battle-tested    │ DeepSeek only        │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Community            │ Large (DDN, labs,│ Small (DeepSeek)     │
│                      │ vendors)         │                      │
├──────────────────────┼──────────────────┼──────────────────────┤
│ Best for             │ General HPC,     │ RDMA-native AI       │
│                      │ mixed workloads  │ clusters             │
└──────────────────────┴──────────────────┴──────────────────────┘

Where Lustre wins:
  - Ecosystem: tools, monitoring, cloud support (FSx)
  - Flexibility: works on any hardware, any network
  - Track record: proven at exascale (Frontier: 35 PB Lustre)
  - PFL: smart auto-tiered striping per file region

Where 3FS wins:
  - RDMA efficiency: one-sided reads bypass storage CPU
  - Self-healing: automatic re-replication on failure
  - Metadata failover: Raft is faster than active-passive
  - Read scaling: CRAQ reads from all replicas, not just one OST
  - Simpler consistency: chain replication vs distributed locks
```

---

## 15. Key Numbers

```
Typical large Lustre deployment:

  OSSes:                     50-200 servers
  OSTs per OSS:              2-4 (each an NVMe or HDD RAID array)
  MDSes:                     1-2 (with DNE for parallel creates)
  Aggregate read bandwidth:  500 GB/s - 2+ TB/s
  Aggregate write bandwidth: 300 GB/s - 1+ TB/s
  Single-client read:        10-25 GB/s (InfiniBand dependent)
  Single-client write:       5-15 GB/s
  Metadata ops (single MDS): 100K-300K ops/sec (stat/open)
  File create rate:          50K-100K creates/sec per MDS
  Stripe size:               1 MB default (configurable)
  Max stripe count:          2000 (practical: match OST count)
  Max file size:              ~32 PB (stripe_count × OST capacity)
  Max filesystem size:        ~500 PB+
  Network:                   InfiniBand (HDR/NDR) or TCP
  Client:                    Linux kernel module
  Backend FS:                ldiskfs (fast) or ZFS (checksums)

  Reference deployments:
    Frontier (ORNL):    35 PB, 480 OSSes, ~5 TB/s read
    El Capitan (LLNL):  ~30 PB Lustre
    Meta AI:            Multiple Lustre clusters for LLaMA training
    AWS FSx for Lustre: Managed Lustre, 100s of GB/s per filesystem
```
