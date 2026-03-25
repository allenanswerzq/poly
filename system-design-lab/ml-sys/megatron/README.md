# Megatron-LM — How to Train the Largest Models

## What It Is

Megatron-LM (NVIDIA) is the framework used to train GPT-3-scale models. Its key contribution: **intra-layer tensor parallelism** — splitting individual layers across GPUs, not just splitting data or pipeline stages.

## The Problem Megatron Solves

```
DDP:  model must fit on 1 GPU → limited to ~10B params on 80GB A100
FSDP: shard parameters, but AllGather full layer each time → bandwidth heavy
Megatron TP: each GPU always holds part of each layer → less communication

For GPT-3 (175B params):
  Single linear layer: 12288 × 49152 = ~2.4GB (fp16)
  With TP=8: each GPU holds 12288 × 6144 = ~300MB
  No AllGather needed: each GPU computes its part independently,
  only a small AllReduce at the end of each layer.
```

## Tensor Parallelism — How Megatron Splits a Layer

```
MLP: Y = GeLU(XA) · B

Column parallel (split A by columns):
  A = [A1 | A2 | A3 | A4]  (split into 4 GPUs)

  GPU 0: Y0 = GeLU(X · A1)    → (seq, hidden/4)
  GPU 1: Y1 = GeLU(X · A2)    → (seq, hidden/4)
  GPU 2: Y2 = GeLU(X · A3)    → (seq, hidden/4)
  GPU 3: Y3 = GeLU(X · A4)    → (seq, hidden/4)

  GeLU is element-wise → applied BEFORE gathering → no communication!

Row parallel (split B by rows):
  B = [B1]    GPU 0: Z0 = Y0 · B1
      [B2]    GPU 1: Z1 = Y1 · B2
      [B3]    GPU 2: Z2 = Y2 · B3
      [B4]    GPU 3: Z3 = Y3 · B4

  Final output: Z = Z0 + Z1 + Z2 + Z3  → AllReduce (sum)

Communication per MLP: 1 AllReduce (just the output, not the weights!)
This is much cheaper than FSDP's AllGather of full layer weights.
```

### Attention Parallelism

```
Multi-head attention with 32 heads, TP=8:
  Each GPU handles 4 attention heads.
  Q, K, V projections split by columns (like MLP column parallel).
  Output projection split by rows.
  1 AllReduce per attention layer.

Total communication per transformer block:
  2 AllReduces: 1 for attention output + 1 for MLP output
  Size: 2 × batch × seq_len × hidden × 4 bytes

With NVLink (900 GB/s), hidden=12288, seq=2048, batch=1:
  2 × 2048 × 12288 × 4 = 200MB → 200MB / 900GB/s = 0.22ms per block
  GPT-3 has 96 blocks → 96 × 0.22ms = 21ms overhead per step
  Training step takes ~2 seconds → 21ms is only 1% overhead. Excellent.
```

## 3D Parallelism — Megatron's Full Strategy

```
The recipe for training 100B+ models:

  TP (Tensor Parallel):   within a node, split layers across 8 GPUs
  PP (Pipeline Parallel):  across nodes, split model into stages
  DP (Data Parallel):      replicate across groups, AllReduce gradients

  Example: 512 GPUs for a 175B model
    TP = 8  (1 node = 8 GPUs, tensor parallel)
    PP = 8  (8 pipeline stages across 8 nodes)
    DP = 8  (8 data parallel replicas)
    Total = 8 × 8 × 8 = 512 GPUs

  Communication:
    TP: 2 AllReduces per block, using NVLink (fast, within node)
    PP: send activations between stages, using InfiniBand
    DP: AllReduce gradients across replicas, using InfiniBand

  Memory per GPU (175B, TP=8, PP=8):
    Parameters:  175B / 8 (TP) / 8 (PP) = ~2.7B params per GPU
    2.7B × 2 bytes = 5.4 GB for parameters
    + optimizer, gradients, activations → ~40-60 GB per GPU → fits in 80GB!
```

## Sequence Parallelism (Megatron SP)

```
Even with TP, some operations are replicated across all GPUs:
  LayerNorm, Dropout, residual add → each GPU computes on the FULL sequence

Sequence Parallel: split these operations along the sequence dimension
  GPU 0: LayerNorm(sequence[0:512])
  GPU 1: LayerNorm(sequence[512:1024])
  ...

  Saves ~20% activation memory with minimal communication overhead.
  The AllReduce in TP becomes an AllGather (same cost).
```

## Megatron vs FSDP vs DeepSpeed

```
                     Megatron-TP    FSDP/ZeRO-3     DDP
Communication type   AllReduce      AllGather        AllReduce
Communication freq   2× per block   2× per block     1× per step
What's communicated  outputs (small) full layer (big) gradients (medium)
Best interconnect    NVLink (must)   InfiniBand (ok)  InfiniBand (ok)
Scaling              8-way TP max    unlimited        unlimited
Complexity           highest         medium           lowest

In practice: Megatron TP × FSDP DP is common
  (TP within node for efficiency, FSDP across nodes for memory)
```
