# Megatron-LM — Large-Scale Model Parallelism

---

## 1. What Megatron Does

```
Megatron-LM (NVIDIA) solves: how do you split a transformer
across GPUs when it doesn't fit on one?

  70B model in BF16 = 140 GB (weights alone).
  H100 has 80 GB.
  → Model doesn't fit. Must split it.

  Megatron provides two splitting strategies:
    1. TENSOR PARALLELISM (TP): split individual layers across GPUs.
    2. PIPELINE PARALLELISM (PP): split groups of layers across GPUs.

  Combined with Data Parallelism (DP), this gives 3D parallelism.
  Megatron is the reference implementation of all three for transformers.

  Used by: NVIDIA, OpenAI (fork), DeepSeek, Mistral, most labs
  that train models >30B parameters.
```

---

## 2. Tensor Parallelism — Splitting a Layer

```
A transformer's main compute is two big matrix multiplies per layer:

  Attention:  Q, K, V projections — [hidden × hidden] @ input
  MLP:        two linear layers — [hidden × 4×hidden] and [4×hidden × hidden]

For a 70B model: hidden = 8192, MLP intermediate = 32768.
  MLP weight matrix: [8192 × 32768] × 2 bytes (BF16) = 512 MB.
  One layer total: ~1.5 GB. 80 layers = 120 GB.

TENSOR PARALLELISM splits these matrices across GPUs:

  MLP First Linear (column parallel):
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Weight matrix W: [8192 × 32768]                            │
  │                                                              │
  │  Split by COLUMNS across 8 GPUs:                            │
  │                                                              │
  │  GPU 0: W[:, 0:4096]        → [8192 × 4096]               │
  │  GPU 1: W[:, 4096:8192]     → [8192 × 4096]               │
  │  GPU 2: W[:, 8192:12288]    → [8192 × 4096]               │
  │  ...                                                        │
  │  GPU 7: W[:, 28672:32768]   → [8192 × 4096]               │
  │                                                              │
  │  Each GPU computes:  Y_chunk = X @ W_chunk                 │
  │    Input X is the SAME on all GPUs (replicated).            │
  │    Each GPU gets a different slice of the output.           │
  │                                                              │
  │  Result: GPU 0 has Y[:, 0:4096]                             │
  │          GPU 1 has Y[:, 4096:8192]                          │
  │          ...                                                │
  │  No communication needed yet!                               │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  MLP Second Linear (row parallel):
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Weight matrix W2: [32768 × 8192]                           │
  │                                                              │
  │  Split by ROWS across 8 GPUs:                               │
  │                                                              │
  │  GPU 0: W2[0:4096, :]       → [4096 × 8192]               │
  │  GPU 1: W2[4096:8192, :]    → [4096 × 8192]               │
  │  ...                                                        │
  │                                                              │
  │  Each GPU computes:  Z_partial = Y_chunk @ W2_chunk         │
  │    Input Y_chunk is the output from column parallel above.  │
  │    Each GPU gets a PARTIAL result (needs to be summed).     │
  │                                                              │
  │  ★ ALLREDUCE HERE ★                                         │
  │  Z_final = AllReduce(Z_partial) across all 8 GPUs.          │
  │  Now every GPU has the complete output.                     │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

The pattern: column-parallel → activation → row-parallel → AllReduce.
  Only ONE AllReduce per MLP block (not per matrix multiply).
  Megatron arranges the splits to minimize communication.

  Same idea for attention:
    Q, K, V projections are column-parallel (split by attention heads).
    Output projection is row-parallel.
    One AllReduce per attention block.

  Total communication per transformer layer:
    2 AllReduces in forward (attention + MLP)
    2 AllReduces in backward
    = 4 AllReduces per layer.

  For 80 layers: 320 AllReduces per training step.
  MUST be over NVLink (900 GB/s). Too frequent for InfiniBand.
```

---

## 3. Pipeline Parallelism — Splitting by Layers

```
Split the model into STAGES. Each stage = a group of layers.
Each stage lives on a different set of GPUs.

  80-layer model, 4 pipeline stages:
    Stage 0 (Node 0): layers 0-19   + embedding
    Stage 1 (Node 1): layers 20-39
    Stage 2 (Node 2): layers 40-59
    Stage 3 (Node 3): layers 60-79  + output head

  Data flows: Stage 0 → Stage 1 → Stage 2 → Stage 3 (forward)
              Stage 3 → Stage 2 → Stage 1 → Stage 0 (backward)

  Communication: only ACTIVATIONS between stages.
    Send activation tensor from stage N to stage N+1.
    Size: [batch_size × seq_len × hidden_dim] × 2 bytes
    For batch=1, seq=8192, hidden=8192: 128 MB per transfer.
    Over InfiniBand (50 GB/s): ~2.5 ms. Acceptable.


THE PIPELINE BUBBLE PROBLEM:

  Naive pipeline: stages sit idle most of the time.

    Stage 0: [  F  ][                    ][  B  ]
    Stage 1:        [  F  ][            ][  B  ]
    Stage 2:               [  F  ][    ][  B  ]
    Stage 3:                      [  F  ][  B  ]
                                         ↑
    F = forward, B = backward.        Only one stage
    Most of the time, 3 of 4 stages   active at a time!
    are idle. 75% wasted compute.     (pipeline "bubble")


MICROBATCHING (GPipe style):

  Split the batch into M microbatches.
  Pipeline processes them one after another.

    Batch of 32 → 4 microbatches of 8.

    Stage 0: [F0][F1][F2][F3][        ][B3][B2][B1][B0]
    Stage 1:     [F0][F1][F2][F3][    ][B3][B2][B1][B0]
    Stage 2:         [F0][F1][F2][F3  ][B3][B2][B1][B0]
    Stage 3:             [F0][F1][F2][F3][B3][B2][B1][B0]

    Better! But still a bubble at the start and end.
    Bubble fraction = (P-1) / (P-1+M) where P=stages, M=microbatches.
    With P=4, M=8: bubble = 3/11 = 27%. Still significant.


1F1B SCHEDULE (Megatron's approach):

  "One Forward, One Backward" — interleave F and B.

    Stage 0: [F0][F1][F2][F3][B0][F4][B1][F5][B2][B3][B4][B5]
    Stage 1:     [F0][F1][F2][B0][F3][B1][F4][B2][F5][B3][B4][B5]
    Stage 2:         [F0][F1][B0][F2][B1][F3][B2][F4][B3][F5][B4][B5]
    Stage 3:             [F0][B0][F1][B1][F2][B2][F3][B3][F4][B4][F5][B5]

    After the pipeline fills, each stage alternates F and B.
    KEY BENEFIT: memory is bounded.
      At any time, each stage has at most P in-flight microbatches.
      GPipe stores ALL M microbatches' activations simultaneously.
      1F1B frees activations as soon as backward finishes for each.

    Bubble fraction: same as GPipe = (P-1) / (P-1+M).
    But MEMORY is much lower → can use larger microbatches.


INTERLEAVED PIPELINE (Megatron v2):

  Give each GPU MULTIPLE non-contiguous stages.

    Virtual pipeline stages = 2× physical stages.
    GPU 0: layers 0-9, layers 40-49    (stages 0 and 4)
    GPU 1: layers 10-19, layers 50-59  (stages 1 and 5)
    GPU 2: layers 20-29, layers 60-69  (stages 2 and 6)
    GPU 3: layers 30-39, layers 70-79  (stages 3 and 7)

    Bubble fraction = (P-1) / (P-1+V×M)
    where V = virtual stages per GPU.
    With V=2, P=4, M=8: bubble = 3/19 = 16%.
    Much smaller bubble! But more communication.
```

---

## 4. Sequence Parallelism

```
TP splits the weight matrices but NOT the activations of LayerNorm
and Dropout. These still need the full hidden dimension.

Problem: LayerNorm needs the full [batch × seq × hidden] tensor.
  With TP=8, each GPU has 1/8 of hidden after the column-parallel split.
  For LayerNorm, you'd need to AllGather to get the full tensor.

Megatron's Sequence Parallelism:
  For LayerNorm and Dropout: split along the SEQUENCE dimension
  instead of the hidden dimension.

  GPU 0: activations for tokens [0:1024]    (full hidden dim)
  GPU 1: activations for tokens [1024:2048] (full hidden dim)
  ...

  Each GPU can compute LayerNorm locally (has full hidden for its tokens).
  Before the attention/MLP (which needs TP split):
    ReduceScatter → convert from sequence-split to hidden-split.
  After the attention/MLP:
    AllGather → convert back to sequence-split.

  Net effect: no extra communication — just REPLACES the AllReduce
  with ReduceScatter + AllGather (which is the same total bytes).
  But activation memory is reduced by TP factor for non-TP ops.
```

---

## 5. Context Parallelism (Long Sequences)

```
For very long sequences (>8K tokens), even the ACTIVATIONS are huge.

  Sequence 128K tokens, hidden 8192, BF16:
    Activation per layer: 128K × 8192 × 2 bytes = 2 GB per layer.
    80 layers: 160 GB of activations. Doesn't fit on one GPU.

Context Parallelism (CP): split the sequence across GPUs.

  CP = 4:
    GPU 0: tokens [0:32K]
    GPU 1: tokens [32K:64K]
    GPU 2: tokens [64K:96K]
    GPU 3: tokens [96K:128K]

  Self-attention needs ALL tokens to attend to ALL other tokens.
  Solution: Ring Attention.
    Each GPU computes attention for its chunk of queries
    against ALL key-value pairs.
    KV pairs are passed around in a ring (like ring AllReduce).
    Each step, each GPU receives the next chunk of KV from its
    neighbor and computes partial attention.

  Communication: send KV chunks around the ring.
  Overlaps with computation (compute attention while receiving next KV).
```

---

## 6. The Complete 3D+SP+CP Configuration

```
LLaMA-3 405B on 16,384 H100s:

  Tensor Parallel (TP) = 8    (within one node, NVLink)
  Pipeline Parallel (PP) = 16  (across nodes, InfiniBand)
  Data Parallel (DP) = 128     (across replicas)
  Context Parallel (CP) = 2    (split long sequences)

  8 × 16 × 128 = 16,384 GPUs ✓ (CP doubles some groups)

  ┌────────────────────────────────────────────────────────────┐
  │ Parallelism │ Splits what       │ Comms pattern │ Network  │
  ├─────────────┼───────────────────┼───────────────┼──────────┤
  │ TP = 8      │ Weight matrices   │ AllReduce     │ NVLink   │
  │             │ (columns/rows)    │ every layer   │ (900GB/s)│
  ├─────────────┼───────────────────┼───────────────┼──────────┤
  │ PP = 16     │ Layer groups      │ P2P send/recv │ IB       │
  │             │ (stages)          │ between stages│ (50GB/s) │
  ├─────────────┼───────────────────┼───────────────┼──────────┤
  │ DP = 128    │ Batch (data)      │ AllReduce     │ IB       │
  │             │ (each replica     │ once per step │ (overlap │
  │             │  gets diff data)  │ (gradients)   │  w/ back)│
  ├─────────────┼───────────────────┼───────────────┼──────────┤
  │ SP          │ Activation seqlen │ Scatter/Gather│ NVLink   │
  │             │ (LayerNorm/Drop)  │ (fused w/ TP) │          │
  ├─────────────┼───────────────────┼───────────────┼──────────┤
  │ CP = 2      │ Sequence (KV)     │ Ring send/recv│ IB       │
  │             │ (long contexts)   │ (ring attn)   │          │
  └─────────────┴───────────────────┴───────────────┴──────────┘
```

---

## 7. Activation Checkpointing (Recomputation)

```
Problem: storing all activations for backward pass uses too much memory.

  Forward pass through 80 layers produces 80 layers of activations.
  Each needed for backward. Total: tens of GB per GPU.

Solution: DON'T store all activations. Recompute them during backward.

  Full checkpointing:
    Forward: compute all layers, but only SAVE activations at
    checkpointed layers (e.g., every 10 layers).
    Backward: when you need layer 15's activation, recompute
    layers 11-15 from the checkpoint at layer 10.

    Memory: reduced by ~10× (only store every 10th layer).
    Compute: ~33% extra (recompute each layer once in backward).

  Selective checkpointing (Megatron default):
    Checkpoint only the EXPENSIVE activations (attention scores).
    Recompute only attention (which is cheap relative to memory saved).
    Much less overhead than full checkpointing.

  Memory savings:
    Without checkpointing:  ~50 GB activation memory per GPU
    With selective:         ~10-15 GB per GPU
    With full:             ~5 GB per GPU (but 33% slower)
```

---

## 8. Key Numbers

```
Megatron-LM performance (H100 cluster):

  70B model, TP=8, PP=4, DP=16 (512 GPUs):
    ~180 TFLOPS per GPU (MFU ~36%)
    ~40K tokens/sec throughput

  405B model, TP=8, PP=16, DP=128 (16K GPUs):
    ~380-425 TFLOPS per GPU (MFU ~38-43%)
    ~16M tokens/sec throughput

  Pipeline bubble overhead:
    With 1F1B, 16 stages, 64 microbatches: ~19% bubble
    With interleaved (V=2): ~13% bubble

  Tensor parallel AllReduce per layer:
    ~128 MB per AllReduce (hidden=8192, batch×seq=4096×8192)
    Over NVLink (450 GB/s effective): ~0.3 ms
    80 layers × 4 AllReduces × 0.3 ms = ~96 ms per step
    (this overlaps with computation in practice)
```
