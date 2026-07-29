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
```

### 3.1 Setup — Split Each Buffer Into N Chunks

```
4 GPUs, each has a gradient buffer [a, b, c, d] (4 chunks).
Using small numbers for clarity:

  GPU 0: [a0, b0, c0, d0] = [1,  5,  9, 13]
  GPU 1: [a1, b1, c1, d1] = [2,  6, 10, 14]
  GPU 2: [a2, b2, c2, d2] = [3,  7, 11, 15]
  GPU 3: [a3, b3, c3, d3] = [4,  8, 12, 16]

  Goal after AllReduce(SUM): every GPU has [10, 26, 42, 58]
    (1+2+3+4=10, 5+6+7+8=26, 9+10+11+12=42, 13+14+15+16=58)

  The ring (clockwise):  GPU0 → GPU1 → GPU2 → GPU3 → GPU0
```

### 3.2 Phase 1: Reduce-Scatter (3 steps for 4 GPUs)

```
Each GPU sends ONE chunk clockwise and RECEIVES one from its left neighbor.
The received chunk is ADDED to its own chunk in that position.

───────────────────────────────────────────────────────────────────
STEP 0:  Each GPU sends chunk[gpu_id] clockwise.
───────────────────────────────────────────────────────────────────

        sends chunk 0          sends chunk 1
  GPU0 ──────────────► GPU1 ──────────────► GPU2
    ▲                                          │
    │    sends chunk 3          sends chunk 2  │
  GPU3 ◄──────────────── GPU3 ◄────────────────┘
                          (wait, let me draw this properly)

  GPU 0: sends a0 (=1)  → GPU 1,   receives d3 (=16) from GPU 3
  GPU 1: sends b1 (=6)  → GPU 2,   receives a0 (=1)  from GPU 0
  GPU 2: sends c2 (=11) → GPU 3,   receives b1 (=6)  from GPU 1
  GPU 3: sends d3 (=16) → GPU 0,   receives c2 (=11) from GPU 2

  After receiving, ADD to own chunk at that position:

  GPU 0: [a0,  b0,  c0,  d0+d3]  = [ 1,  5,  9,  29]
                              ^^     chunk d now has 13+16=29
  GPU 1: [a1+a0, b1,  c1,  d1]   = [ 3,  6, 10,  14]
          ^^                         chunk a now has 2+1=3
  GPU 2: [a2,  b2+b1, c2,  d2]   = [ 3, 13, 11,  15]
               ^^                    chunk b now has 7+6=13
  GPU 3: [a3,  b3,  c3+c2, d3]   = [ 4,  8, 23,  16]
                    ^^               chunk c now has 12+11=23

───────────────────────────────────────────────────────────────────
STEP 1:  Each GPU sends its UPDATED chunk one position clockwise.
         (send the chunk that was just accumulated)
───────────────────────────────────────────────────────────────────

  GPU 0: sends d0+d3 (=29)  → GPU 1,  receives c3+c2 (=23) from GPU 3
  GPU 1: sends a1+a0 (=3)   → GPU 2,  receives d0+d3 (=29) from GPU 0
  GPU 2: sends b2+b1 (=13)  → GPU 3,  receives a1+a0 (=3)  from GPU 1
  GPU 3: sends c3+c2 (=23)  → GPU 0,  receives b2+b1 (=13) from GPU 2

  After ADD:

  GPU 0: [ 1,  5,  9+23,  29]    = [ 1,  5, 32,  29]
                   ^^                  chunk c has 9+23=32 (3 of 4 values)
  GPU 1: [ 3,  6, 10,  14+29]    = [ 3,  6, 10,  43]
                        ^^            chunk d has 14+29=43 (3 of 4 values)
  GPU 2: [3+3, 13, 11,  15]      = [ 6, 13, 11,  15]
          ^^                          chunk a has 3+3=6 (3 of 4 values)
  GPU 3: [ 4, 8+13, 23,  16]     = [ 4, 21, 23,  16]
              ^^                      chunk b has 8+13=21 (3 of 4 values)

───────────────────────────────────────────────────────────────────
STEP 2:  One more rotation. (last step of reduce-scatter)
───────────────────────────────────────────────────────────────────

  GPU 0: sends c (=32) → GPU 1,  receives b (=21) from GPU 3
  GPU 1: sends d (=43) → GPU 2,  receives c (=32) from GPU 0
  GPU 2: sends a (=6)  → GPU 3,  receives d (=43) from GPU 1
  GPU 3: sends b (=21) → GPU 0,  receives a (=6)  from GPU 2

  After ADD:

  GPU 0: [ 1, 5+21, 32,  29]     = [ 1, ★26, 32,  29]
                                       chunk b = 5+21 = 26 ✓ COMPLETE
  GPU 1: [ 3,  6, 10+32, 43]     = [ 3,  6, ★42,  43]
                                       chunk c = 10+32 = 42 ✓ COMPLETE
  GPU 2: [ 6, 13, 11,  15+43]    = [ 6, 13, 11, ★58]
                                       chunk d = 15+43 = 58 ✓ COMPLETE
  GPU 3: [4+6, 21, 23,  16]      = [★10, 21, 23,  16]
                                       chunk a = 4+6 = 10 ✓ COMPLETE

REDUCE-SCATTER DONE. Each GPU has ONE fully-reduced chunk (marked ★):
  GPU 0: [ _,  26,  _,  _]   ← owns chunk b (fully reduced)
  GPU 1: [ _,   _, 42,  _]   ← owns chunk c
  GPU 2: [ _,   _,  _, 58]   ← owns chunk d
  GPU 3: [10,   _,  _,  _]   ← owns chunk a
```

### 3.3 Phase 2: AllGather (3 more steps)

```
Same ring, same direction. But now just OVERWRITE (no addition).
Each GPU sends its completed chunk around the ring.

───────────────────────────────────────────────────────────────────
STEP 3:
───────────────────────────────────────────────────────────────────
  GPU 0: sends 26 (chunk b) → GPU 1,  receives 10 (chunk a) from GPU 3
  GPU 1: sends 42 (chunk c) → GPU 2,  receives 26 (chunk b) from GPU 0
  GPU 2: sends 58 (chunk d) → GPU 3,  receives 42 (chunk c) from GPU 1
  GPU 3: sends 10 (chunk a) → GPU 0,  receives 58 (chunk d) from GPU 2

  GPU 0: [10, 26,  _,  _]
  GPU 1: [ _, 26, 42,  _]
  GPU 2: [ _,  _, 42, 58]
  GPU 3: [10,  _,  _, 58]

───────────────────────────────────────────────────────────────────
STEP 4:
───────────────────────────────────────────────────────────────────
  GPU 0: sends 10 → GPU 1,  receives 58 from GPU 3
  GPU 1: sends 26 → GPU 2,  receives 10 from GPU 0
  GPU 2: sends 42 → GPU 3,  receives 26 from GPU 1
  GPU 3: sends 58 → GPU 0,  receives 42 from GPU 2

  GPU 0: [10, 26,  _, 58]
  GPU 1: [10, 26, 42,  _]
  GPU 2: [ _, 26, 42, 58]
  GPU 3: [10,  _, 42, 58]

───────────────────────────────────────────────────────────────────
STEP 5:  (final step)
───────────────────────────────────────────────────────────────────
  GPU 0: sends 58 → GPU 1,  receives 42 from GPU 3
  GPU 1: sends 10 → GPU 2,  receives 58 from GPU 0
  GPU 2: sends 26 → GPU 3,  receives 10 from GPU 1
  GPU 3: sends 42 → GPU 0,  receives 26 from GPU 2

  GPU 0: [10, 26, 42, 58]  ✓
  GPU 1: [10, 26, 42, 58]  ✓
  GPU 2: [10, 26, 42, 58]  ✓
  GPU 3: [10, 26, 42, 58]  ✓

ALLREDUCE COMPLETE!

Every GPU now has [10, 26, 42, 58] = sum of all original buffers.
Total steps: 2 × (4-1) = 6.
Each step: each GPU sends and receives exactly 1 chunk (100 MB).
```

### 3.4 Why Ring AllReduce Is Bandwidth-Optimal

```
  ┌───────────────────────────────────────────────────────────────┐
  │                                                               │
  │  Per GPU, total data sent:                                   │
  │    Phase 1 (reduce-scatter): (N-1) steps × (data/N) per step│
  │      = (N-1)/N × data ≈ data    (for large N)               │
  │                                                               │
  │    Phase 2 (allgather): same                                 │
  │      = (N-1)/N × data ≈ data                                │
  │                                                               │
  │    Total per GPU: ~2 × data                                  │
  │                                                               │
  │  This is the SAME whether N=4 or N=4000.                    │
  │  Adding more GPUs does NOT increase per-GPU bandwidth usage. │
  │  The only thing that grows is LATENCY (more steps).          │
  │                                                               │
  │  Time = 2 × (N-1) × latency + 2 × data / bandwidth         │
  │           ^^^^^^^^^^^^^^^^^^   ^^^^^^^^^^^^^^^^^^^^^^^^      │
  │           grows with N          constant (bandwidth term)    │
  │           (but small for        (dominates for large data)   │
  │            large messages)                                    │
  │                                                               │
  │  For ML training (gradients are 100s of MB):                 │
  │    bandwidth term dominates → ring is near-optimal.          │
  │                                                               │
  └───────────────────────────────────────────────────────────────┘

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

  Each link carries exactly data/N per step.
  All links active simultaneously → full bisection bandwidth used.
  No single GPU is a bottleneck (unlike naive "send to GPU 0").
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

---

## 9. How the GPU Schedules and Runs CUDA Streams

### 9.1 GPU Hardware Engines

```
The GPU has HARDWARE QUEUES, not a software scheduler.
Unlike a CPU (where the OS picks threads), the GPU has fixed hardware
engines that pull work from queues:

  ┌──────────────────────────────────────────────────────────────┐
  │  GPU HARDWARE                                                │
  │                                                              │
  │  ┌────────────────────────────────────────────────────────┐ │
  │  │ HOST INTERFACE (receives commands from CPU via PCIe)   │ │
  │  └────────────────────┬───────────────────────────────────┘ │
  │                       │                                      │
  │                       ▼                                      │
  │  ┌──────────────────────────────────────────────────────┐   │
  │  │ COMMAND PROCESSOR (front-end)                        │   │
  │  │                                                      │   │
  │  │ Receives commands from CPU driver, dispatches to     │   │
  │  │ the correct hardware engine.                         │   │
  │  │                                                      │   │
  │  │ Hardware Work Queues:                                │   │
  │  │   ┌───────────────────┐                              │   │
  │  │   │ Compute Engine(s) │ ← kernel launches go here   │   │
  │  │   │ (GPC/SM dispatch) │                              │   │
  │  │   └───────────────────┘                              │   │
  │  │   ┌───────────────────┐                              │   │
  │  │   │ Copy Engine 0     │ ← H→D memcpy goes here     │   │
  │  │   │ (DMA controller)  │                              │   │
  │  │   └───────────────────┘                              │   │
  │  │   ┌───────────────────┐                              │   │
  │  │   │ Copy Engine 1     │ ← D→H memcpy goes here     │   │
  │  │   │ (DMA controller)  │                              │   │
  │  │   └───────────────────┘                              │   │
  │  │                                                      │   │
  │  └──────────────────────────────────────────────────────┘   │
  │                                                              │
  │  These engines run INDEPENDENTLY and CONCURRENTLY.          │
  │  Copy Engine 0 can DMA while Compute Engine runs a kernel.  │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

### 9.2 How CUDA Streams Map to Hardware

```
CUDA streams are SOFTWARE queues on the CPU side.
The GPU driver translates them into hardware queue submissions.

  CPU side (your code):              GPU side (hardware):

  Stream A: [kernel1, kernel2]       Compute Engine queue:
  Stream B: [memcpy_H2D, kernel3]     [kernel1, kernel3, kernel2]
  Stream C: [memcpy_D2H]              (interleaved from all streams)

                                     Copy Engine 0 queue:
                                       [memcpy_H2D]

                                     Copy Engine 1 queue:
                                       [memcpy_D2H]

  The driver's job:
    1. Take CUDA stream commands
    2. Send kernels → compute engine queue
    3. Send H→D copies → copy engine 0 queue
    4. Send D→H copies → copy engine 1 queue
    5. Insert DEPENDENCIES (from stream ordering) as hardware fences

  Within a stream: operations are ordered (A before B before C).
  Across streams: no ordering UNLESS you add explicit sync.
```

### 9.3 The Compute Engine — How Kernels Run

```
When a kernel reaches the front of the compute engine queue:

  1. GRID SCHEDULER reads the launch config:
     kernel<<<grid(256), block(256), shared_mem, stream>>>
     → 256 thread blocks, 256 threads each

  2. GRID SCHEDULER dispatches thread blocks to GPCs
     (Graphics Processing Clusters, each containing multiple SMs)

     ┌──────────────────────────────────────────────────────┐
     │  GPC 0           GPC 1           GPC 2    ...       │
     │  ┌────┐┌────┐   ┌────┐┌────┐   ┌────┐┌────┐       │
     │  │SM 0││SM 1│   │SM 2││SM 3│   │SM 4││SM 5│       │
     │  └────┘└────┘   └────┘└────┘   └────┘└────┘       │
     │                                                     │
     │  Grid Scheduler assigns blocks round-robin:        │
     │    Block 0 → SM 0                                  │
     │    Block 1 → SM 1                                  │
     │    ...                                              │
     │    Block 107 → SM 107                              │
     │    Block 108 → SM 0 (wraps, if SM 0 is done)      │
     │                                                     │
     │  An SM can run multiple blocks simultaneously      │
     │  (if it has enough registers + shared memory).     │
     └──────────────────────────────────────────────────────┘

  3. Inside each SM, WARP SCHEDULERS manage execution:
     Block of 256 threads = 8 warps of 32 threads.
     SM has 4 warp schedulers. Each cycle, each scheduler picks
     one READY warp and issues its next instruction.

     ┌────────────────────────────────────────────────┐
     │  SM (one Streaming Multiprocessor)              │
     │                                                │
     │  Warp Scheduler 0 → picks warp, issues insn   │
     │  Warp Scheduler 1 → picks warp, issues insn   │
     │  Warp Scheduler 2 → picks warp, issues insn   │
     │  Warp Scheduler 3 → picks warp, issues insn   │
     │                                                │
     │  Warps ready?                                  │
     │    Warp 0: READY   ← scheduler picks this     │
     │    Warp 1: STALLED (waiting for memory load)   │
     │    Warp 2: READY   ← scheduler picks this     │
     │    Warp 3: STALLED                             │
     │    Warp 4: READY                               │
     │    ...                                          │
     │                                                │
     │  Switching between warps is FREE.              │
     │  No context switch cost (all state in registers│
     │  simultaneously). This is how GPUs hide latency│
     │  — while one warp waits for memory (~400 cycles│
     │  another warp runs. No cycles wasted.          │
     └────────────────────────────────────────────────┘
```

### 9.4 Concurrent Kernels from Different Streams

```
Can two kernels from different streams run at the same time?

  YES — if the GPU has enough SMs to fit both.

  Stream A: kernel_small (needs 4 SMs)
  Stream B: kernel_big   (needs 100 SMs)

  GPU has 108 SMs (H100):
    kernel_small gets SMs [0-3]
    kernel_big   gets SMs [4-107]
    Both run simultaneously.

  But if both kernels need all 108 SMs:
    First kernel fills all SMs.
    Second kernel WAITS until some SMs free up.
    They might overlap partially (as blocks from kernel1 finish,
    blocks from kernel2 start on those SMs).

  This is why NCCL uses separate streams for comms:

    Compute stream: [backward pass kernel]    ← uses ~all SMs
    NCCL stream:    [AllReduce]               ← uses NIC, not SMs!

    AllReduce is mostly NIC + DMA work, not SM compute.
    So it truly runs concurrently with the backward kernel.
    The SM just kicks off the RDMA transfer, then the NIC does the rest.
```

### 9.5 Stream Synchronization — How Ordering Is Enforced

```
Within a stream: GPU hardware guarantees ordering.
  Stream A: [kernel1] → [kernel2]
  kernel2 will NOT start until kernel1 finishes.
  Enforced by hardware fences in the compute engine queue.

Across streams: no ordering by default.
  Stream A: [kernel1]
  Stream B: [kernel2]
  kernel1 and kernel2 may run in any order, or overlap.

To add cross-stream dependencies: CUDA Events.

  cudaEvent_t event;
  cudaEventCreate(&event);

  // Stream A does work, then records an event
  kernel1<<<..., streamA>>>();
  cudaEventRecord(event, streamA);   // "mark this point in stream A"

  // Stream B waits for that event before proceeding
  cudaStreamWaitEvent(streamB, event);  // "don't run until event fires"
  kernel2<<<..., streamB>>>();

  Timeline:
    Stream A: [kernel1]──●event
    Stream B:       wait...[kernel2]
                          ↑
                   starts only after event fires

Under the hood: cudaEventRecord inserts a hardware semaphore write.
cudaStreamWaitEvent inserts a hardware semaphore wait.
No CPU involvement once submitted — the GPU enforces it.


THE DEFAULT STREAM TRAP:
  Stream 0 (default stream) implicitly synchronizes with all streams.

  kernel1<<<..., streamA>>>();
  kernel_default<<<...>>>();      // default stream — WAITS for streamA!
  kernel2<<<..., streamB>>>();    // WAITS for default to finish!

  The default stream acts as a barrier. Kills concurrency.
  Rule: never mix default stream with explicit streams.
  Use --default-stream per-thread to avoid this.
```

### 9.6 Summary — The Full Hierarchy

```
  YOUR CODE          CUDA DRIVER         GPU HARDWARE
  ──────────         ───────────         ────────────
  Stream A ────────► driver queue  ────► Compute Engine
  Stream B ────────►               ────► (kernel dispatch)
  Stream C ────────►               ────► Copy Engine 0
                                   ────► Copy Engine 1

  CUDA stream    = software FIFO of commands
  CUDA event     = software sync point → hardware semaphore
  Compute Engine = hardware that dispatches thread blocks to SMs
  Copy Engine    = hardware DMA controller for memcpy
  SM             = hardware that executes warps (32-thread SIMD)
  Warp Scheduler = hardware that picks which warp runs each cycle
```
