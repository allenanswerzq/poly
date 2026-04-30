# NCCL — NVIDIA Collective Communications Library

---

## 1. What NCCL Does

```
NCCL (pronounced "nickel") is the library that moves data between GPUs.

When 16,384 GPUs train a model together, they need to:
  - Average gradients across all GPUs (AllReduce)
  - Gather model shards from all GPUs (AllGather)
  - Scatter results to all GPUs (ReduceScatter)
  - Send activations between pipeline stages (Send/Recv)

NCCL implements these "collective operations" optimized for
NVIDIA GPU hardware — NVLink, NVSwitch, InfiniBand, RoCE.

Without NCCL: you'd write your own GPU networking code.
With NCCL:    one function call. It handles everything.

  ncclAllReduce(send_buf, recv_buf, count, datatype, op, comm, stream);
  // That's it. Averages a buffer across all GPUs in the communicator.
```

---

## 2. The Collective Operations

```
The six operations NCCL provides:

1. ALLREDUCE — the most important one
   ───────────
   Every GPU has a gradient buffer. Compute the SUM (or AVERAGE)
   and deliver the result to ALL GPUs.

   Before:
     GPU 0: [1, 2, 3]
     GPU 1: [4, 5, 6]
     GPU 2: [7, 8, 9]

   AllReduce(SUM):
     GPU 0: [12, 15, 18]    (1+4+7, 2+5+8, 3+6+9)
     GPU 1: [12, 15, 18]    (same result on every GPU)
     GPU 2: [12, 15, 18]

   Used for: Data Parallel gradient averaging.
   Every step, every GPU needs the averaged gradient.


2. ALLGATHER
   ───────────
   Each GPU has a piece. Gather all pieces into every GPU.

   Before:
     GPU 0: [A]
     GPU 1: [B]
     GPU 2: [C]

   AllGather:
     GPU 0: [A, B, C]
     GPU 1: [A, B, C]
     GPU 2: [A, B, C]

   Used for: FSDP/ZeRO — gather full weights before forward pass.


3. REDUCESCATTER
   ───────────────
   Reduce (sum) and scatter the result — each GPU gets one piece.

   Before:
     GPU 0: [1, 2, 3]
     GPU 1: [4, 5, 6]
     GPU 2: [7, 8, 9]

   ReduceScatter(SUM):
     GPU 0: [12]        (1+4+7)
     GPU 1: [15]        (2+5+8)
     GPU 2: [18]        (3+6+9)

   Used for: FSDP/ZeRO — reduce gradients and keep only your shard.
   This is effectively "AllReduce split in half."
   AllReduce = ReduceScatter + AllGather.


4. BROADCAST
   ──────────
   One GPU sends its data to all others.

   Before:
     GPU 0: [A, B, C]    (root)
     GPU 1: [?, ?, ?]
     GPU 2: [?, ?, ?]

   Broadcast(root=0):
     GPU 0: [A, B, C]
     GPU 1: [A, B, C]
     GPU 2: [A, B, C]

   Used for: distributing initial model weights to all GPUs.


5. REDUCE
   ───────
   Like AllReduce but result goes to only ONE GPU.

   ReduceScatter + Gather to root = Reduce.
   Less common in training.


6. SEND / RECV (Point-to-point)
   ─────────────
   One GPU sends to one specific other GPU.

   Used for: Pipeline parallelism — sending activations
   from stage N to stage N+1.
```

---

## 3. How AllReduce Works — The Ring Algorithm

```
The classic NCCL algorithm: RING ALLREDUCE.

Problem: 4 GPUs each have 400 MB of gradients.
Goal: every GPU gets the sum of all gradients.

Naive approach: send everything to GPU 0, sum, broadcast back.
  Total data moved: 4 × 400MB × 2 = 3.2 GB through one GPU.
  GPU 0's bandwidth is the bottleneck. Doesn't scale.

Ring algorithm: arrange GPUs in a ring. Two phases.

  PHASE 1: REDUCE-SCATTER (N-1 steps)
  ────────────────────────────────────
  Split each GPU's buffer into N chunks (4 chunks for 4 GPUs).
  Each step: every GPU sends one chunk clockwise and receives one.
  Accumulate (add) the received chunk to its own.

  Step 0:  GPU0 sends chunk0 → GPU1
           GPU1 sends chunk1 → GPU2
           GPU2 sends chunk2 → GPU3
           GPU3 sends chunk3 → GPU0

  Step 1:  Each GPU sends its updated chunk one more step clockwise.
           GPU0 sends (chunk3+its_chunk3) → GPU1
           GPU1 sends (chunk0+two values) → GPU2
           ...

  Step 2:  One more rotation.

  After 3 steps (N-1):
    Each GPU has ONE fully-reduced chunk.
    GPU 0 has sum of all chunk3s.
    GPU 1 has sum of all chunk0s.
    GPU 2 has sum of all chunk1s.
    GPU 3 has sum of all chunk2s.


  PHASE 2: ALLGATHER (N-1 steps)
  ────────────────────────────────
  Now distribute the completed chunks to all GPUs.
  Same ring, same rotation, but just overwrite (no addition).

  After 3 more steps:
    Every GPU has ALL the fully-reduced chunks.
    = AllReduce complete.


  Total steps: 2 × (N-1) = 6 steps for 4 GPUs.
  Data per step per GPU: 400MB / 4 = 100 MB.
  Total data moved per GPU: 2 × (N-1)/N × 400 MB ≈ 600 MB.

  KEY PROPERTY: bandwidth is INDEPENDENT of number of GPUs.
    Each GPU sends 2 × (N-1)/N × data ≈ 2 × data.
    Whether you have 4 GPUs or 4000, same bandwidth per GPU.
    Time = 2 × data_size / bandwidth_per_link.

  ┌──────┐      ┌──────┐
  │ GPU0 │─────►│ GPU1 │
  │      │◄─────│      │
  └──┬───┘      └───┬──┘
     │    ╲    ╱    │
     │     ╲  ╱     │
     │      ╲╱      │
     │      ╱╲      │
     │     ╱  ╲     │
     │    ╱    ╲    │
  ┌──┴───┐      ┌───┴──┐
  │ GPU3 │◄─────│ GPU2 │
  │      │─────►│      │
  └──────┘      └──────┘
```

---

## 4. Beyond Rings — Tree and Double Binary Tree

```
Ring AllReduce is great for bandwidth but has LATENCY issues:
  N-1 steps means latency grows with number of GPUs.
  For 1000 GPUs: 999 steps × per-step latency = slow start.

Modern NCCL uses TREE algorithms for small messages
and RING for large messages:

  TREE ALLREDUCE:
    Arrange GPUs in a binary tree.
    Reduce up the tree (leaves → root): log₂(N) steps.
    Broadcast down the tree (root → leaves): log₂(N) steps.
    Total: 2 × log₂(N) steps.

    For 1024 GPUs: 2 × 10 = 20 steps (vs 1023 for ring).
    Much lower latency for small messages.
    But: root node carries more traffic. Bandwidth limited.

  NCCL 2.x AUTO-SELECTS:
    Small messages (<256 KB):   tree (latency-optimized)
    Large messages (>256 KB):   ring (bandwidth-optimized)
    Very large across many GPUs: multi-ring or 2D algorithms

  DOUBLE BINARY TREE (NCCL 2.12+):
    Two overlapping binary trees.
    Both trees are active simultaneously.
    Halves the bandwidth bottleneck at the root.
    Best of both: good latency AND good bandwidth.
```

---

## 5. The Hardware: NVLink vs InfiniBand

```
NCCL picks different paths depending on WHERE the GPUs are:

  SAME NODE — NVLink / NVSwitch:
  ┌──────────────────────────────────────────────────────┐
  │  8 H100 GPUs connected via NVSwitch                  │
  │                                                      │
  │  NVSwitch provides ALL-TO-ALL connectivity:          │
  │    Any GPU → any GPU: 900 GB/s bidirectional        │
  │    Total bisection bandwidth: 3.6 TB/s              │
  │                                                      │
  │  NCCL on NVLink:                                     │
  │    Uses NVLink SHARP (in-network reduction)          │
  │    The NVSwitch itself can add numbers!              │
  │    GPU0 sends to NVSwitch, GPU1 sends to NVSwitch,  │
  │    NVSwitch adds them, sends result back.            │
  │    → AllReduce in ONE step (not N-1 steps).          │
  │    → ~450 GB/s effective AllReduce bandwidth.        │
  └──────────────────────────────────────────────────────┘

  ACROSS NODES — InfiniBand / RoCE:
  ┌──────────────────────────────────────────────────────┐
  │  Node A ──── InfiniBand switch ──── Node B           │
  │                                                      │
  │  Each H100 has a dedicated ConnectX-7 NIC:           │
  │    400 Gbps (50 GB/s) per NIC                       │
  │    8 NICs per node = 400 GB/s aggregate              │
  │                                                      │
  │  NCCL on InfiniBand:                                 │
  │    Uses RDMA (GPUDirect RDMA):                      │
  │    GPU memory → NIC → wire → NIC → GPU memory.     │
  │    NO CPU copy. No kernel involvement.               │
  │                                                      │
  │    GDR (GPUDirect RDMA) path:                       │
  │    GPU HBM → PCIe → NIC → IB fabric → NIC → PCIe → GPU HBM
  │                                                      │
  │    Latency: ~1-5 μs                                  │
  │    Bandwidth per link: 50 GB/s                      │
  │    Bandwidth per GPU (1 NIC): 50 GB/s               │
  └──────────────────────────────────────────────────────┘

  NCCL HIERARCHICAL ALLREDUCE (combining both):
  ┌──────────────────────────────────────────────────────┐
  │                                                      │
  │  Step 1: Intra-node reduce (NVLink)                 │
  │    8 GPUs within each node → reduce to 1 value      │
  │    Uses NVLink (900 GB/s). Very fast.               │
  │                                                      │
  │  Step 2: Inter-node AllReduce (InfiniBand)          │
  │    1 GPU per node participates in cross-node reduce │
  │    Uses ring algorithm over InfiniBand.             │
  │    Only 1/8 the data (already reduced intra-node).  │
  │                                                      │
  │  Step 3: Intra-node broadcast (NVLink)              │
  │    The representative GPU broadcasts result to the  │
  │    other 7 GPUs via NVLink.                         │
  │                                                      │
  │  This is MUCH faster than a flat ring across all    │
  │  128 GPUs, because the slow InfiniBand link only    │
  │  carries 1/8 the data.                              │
  │                                                      │
  └──────────────────────────────────────────────────────┘
```

---

## 6. Overlapping Communication with Computation

```
The key to training performance: DON'T wait for AllReduce.

  NAIVE (no overlap):
    forward → backward → AllReduce → optimizer step
                          ^^^^^^^^
                          GPUs idle during compute.
                          Compute idle during comms.

  OVERLAPPED:
    forward → backward starts
              ↓
              first layer's gradients ready → start AllReduce for them
              ↓
              second layer's gradients ready → start AllReduce for them
              ↓
              ... (backward still computing deeper layers)
              ↓
              last layer gradients ready → start AllReduce
              ↓
              first layer's AllReduce already DONE
              ↓
              optimizer step (for layers whose AllReduce finished)

  PyTorch DDP does this automatically:
    Gradients are bucketed (~25 MB per bucket).
    As each bucket fills (backward pass produces gradients),
    NCCL AllReduce starts immediately for that bucket.
    AllReduce runs on a SEPARATE CUDA STREAM from compute.
    GPU compute engine + NIC work simultaneously.

  ┌────────────────────────────────────────────────────┐
  │ Time →                                             │
  │                                                    │
  │ Compute stream: [forward][  backward (computing) ] │
  │ Comms stream:            [AR 1][AR 2][AR 3][AR 4]  │
  │                                                    │
  │ AR = AllReduce for one bucket of gradients         │
  │ Both streams run concurrently on the GPU.          │
  └────────────────────────────────────────────────────┘
```

---

## 7. NCCL Process Groups

```
Not all GPUs need to talk to all other GPUs.

  16,384 GPUs with 3D parallelism:
    TP group: 8 GPUs (within node) — need AllReduce every layer
    PP group: 16 GPUs (pipeline stages) — need Send/Recv
    DP group: 128 GPUs (data replicas) — need AllReduce per step

  NCCL communicators:
    Each group gets its OWN NCCL communicator.
    Operations on different communicators run independently.

    tp_comm = nccl_comm_init(ranks=[0,1,2,3,4,5,6,7])
    dp_comm = nccl_comm_init(ranks=[0,8,16,24,...])
    pp_comm = nccl_comm_init(ranks=[0,128,256,...])

    // TP AllReduce on one stream
    ncclAllReduce(buf, buf, n, float16, sum, tp_comm, tp_stream);

    // DP AllReduce on another stream (concurrent!)
    ncclAllReduce(grad, grad, m, float16, sum, dp_comm, dp_stream);

  This is critical: TP and DP communication can OVERLAP.
```

---

## 8. Key Numbers

```
NVLink (H100, intra-node):
  Bandwidth per GPU pair:     900 GB/s bidirectional
  AllReduce bandwidth:        ~450 GB/s effective
  Latency:                    ~1 μs

InfiniBand NDR (inter-node):
  Bandwidth per link:         400 Gbps = 50 GB/s
  Bandwidth per node (8 NICs): 400 GB/s aggregate
  Latency:                    ~1 μs
  GPUDirect RDMA:             bypasses CPU entirely

RoCE v2 (Meta's choice):
  Bandwidth:                  same as IB (400 Gbps)
  Latency:                    ~2-5 μs (slightly worse than IB)
  Pro:                        uses standard Ethernet switches (cheaper)
  Con:                        needs careful congestion control (DCQCN)

AllReduce scaling:
  Ring algorithm:             bandwidth-optimal, O(N) latency
  Tree algorithm:             O(log N) latency, less bandwidth
  For 1 GB across 128 GPUs:  ~20-50 ms over InfiniBand
  For 1 GB across 8 GPUs:    ~2-3 ms over NVLink
```
