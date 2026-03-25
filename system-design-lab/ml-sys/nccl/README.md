# NCCL — How GPU-to-GPU Communication Works Under the Hood

## What It Is

NCCL (NVIDIA Collective Communications Library) is the low-level library that moves data between GPUs. Every distributed training framework (DDP, FSDP, DeepSpeed) calls NCCL under the hood.

## Why It Matters

Without NCCL, you can't do distributed training. Period. When 1024 GPUs need to average their gradients, NCCL is what makes it happen in milliseconds instead of minutes.

## Core Operations

```
AllReduce — the most important operation in distributed training
  Every GPU has a gradient vector. Result: every GPU gets the AVERAGE.

  Before:  GPU 0: [1, 2, 3]   GPU 1: [4, 5, 6]   GPU 2: [7, 8, 9]
  After:   GPU 0: [4, 5, 6]   GPU 1: [4, 5, 6]   GPU 2: [4, 5, 6]
                    (average of all three)

  Used by: DDP gradient synchronization


AllGather — collect all shards into a complete tensor
  Each GPU has 1/N of the data. Result: every GPU gets ALL the data.

  Before:  GPU 0: [A]   GPU 1: [B]   GPU 2: [C]
  After:   GPU 0: [A,B,C]   GPU 1: [A,B,C]   GPU 2: [A,B,C]

  Used by: FSDP weight gathering before forward pass


ReduceScatter — reduce then scatter in one operation
  Combine AllReduce + Scatter. Result: each GPU gets 1/N of the reduced data.

  Before:  GPU 0: [1,2,3]   GPU 1: [4,5,6]   GPU 2: [7,8,9]
  After:   GPU 0: [12]      GPU 1: [15]       GPU 2: [18]
             (sum of col 0)  (sum of col 1)   (sum of col 2)

  Used by: FSDP gradient reduction after backward pass


Broadcast — one GPU sends to all
  Before:  GPU 0: [data]   GPU 1: []   GPU 2: []
  After:   GPU 0: [data]   GPU 1: [data]   GPU 2: [data]

  Used by: weight initialization, checkpoint loading
```

## Ring AllReduce — The Algorithm

```
The naive approach (bad):
  All GPUs send gradients to GPU 0 → GPU 0 averages → GPU 0 broadcasts
  Problem: GPU 0 is a bottleneck. All N GPUs send to 1 GPU → N× bandwidth needed.

Ring AllReduce (what NCCL actually does):
  Arrange GPUs in a ring. Each GPU sends/receives to its neighbors.

  4 GPUs, gradient size = 4 chunks (A, B, C, D)

  Step 1 (scatter-reduce):
    GPU 0 sends chunk A to GPU 1, receives chunk D from GPU 3
    GPU 1 sends chunk B to GPU 2, receives chunk A from GPU 0
    GPU 2 sends chunk C to GPU 3, receives chunk B from GPU 1
    GPU 3 sends chunk D to GPU 0, receives chunk C from GPU 2
    → Each GPU accumulates partial sums

  After N-1 scatter-reduce steps:
    Each GPU has the COMPLETE sum of 1 chunk

  Step 2 (allgather):
    N-1 more steps, each GPU shares its complete chunk around the ring

  Total data transferred per GPU: 2 × (N-1)/N × M bytes
  For large N: ≈ 2M bytes (independent of number of GPUs!)
  This is why distributed training can scale to thousands of GPUs.

  With NVLink (900 GB/s) and 1GB of gradients:
    Time ≈ 2 × 1GB / 900GB/s ≈ 2.2ms
```

## Transport Hierarchy

```
Within a GPU node (8 GPUs):
  NVLink 4.0: 900 GB/s bidirectional (H100)
  NVSwitch:   full mesh — any GPU talks to any GPU at full bandwidth
  PCIe 5.0:   128 GB/s (fallback, ~7x slower than NVLink)

Between nodes:
  InfiniBand NDR: 400 Gbps = 50 GB/s (18x slower than NVLink!)
  RoCE (RDMA over Ethernet): similar bandwidth, higher latency

NCCL automatically chooses the best transport:
  Same node, NVLink available → NVLink
  Same node, no NVLink → PCIe shared memory
  Different nodes → InfiniBand / TCP

This is why tensor parallelism stays WITHIN a node (needs NVLink speed)
while data parallelism works ACROSS nodes (only needs gradient sync once per step).
```

## NCCL Internals

```
Kernel fusion:
  NCCL fuses small communications into larger ones to amortize launch overhead.
  Instead of 100 separate AllReduce calls for 100 layers:
  → bucket gradients into a few large AllReduce calls.

Tree AllReduce (for large clusters):
  Ring AllReduce has latency O(N) for N GPUs.
  Tree AllReduce: O(log N) latency but 2x bandwidth.
  NCCL uses a hybrid: tree for latency-sensitive small messages,
  ring for bandwidth-sensitive large messages.

Double binary tree:
  Two overlapping binary trees to achieve both low latency AND full bandwidth.

Channel parallelism:
  Multiple rings/trees running simultaneously on different NVLink/IB channels.
  8 channels typical → 8 concurrent data streams.
```
