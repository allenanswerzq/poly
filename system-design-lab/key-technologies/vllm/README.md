# vLLM — High-Throughput LLM Serving Engine

## Overview

vLLM is the **fastest open-source LLM inference engine**. Its key innovation is **PagedAttention** — managing KV cache memory like an OS manages virtual memory. This eliminates memory waste and enables 2-4x higher throughput than naive serving.

## The Problem vLLM Solves

```
Naive LLM serving:
  Allocate max_sequence_length KV cache per request upfront
  ┌──────────────────────────────────────────────────────┐
  │ Request 1: KV cache [████████████░░░░░░░░░░░░░░░░░] │ ← 60% wasted
  │ Request 2: KV cache [████████████████████████░░░░░░] │ ← 25% wasted
  │ Request 3: KV cache [████░░░░░░░░░░░░░░░░░░░░░░░░░] │ ← 85% wasted
  └──────────────────────────────────────────────────────┘
  Average waste: ~50% of GPU memory sitting unused!
  Fewer concurrent requests → lower throughput

vLLM (PagedAttention):
  Allocate KV cache in small pages, on demand
  ┌──────────────────────────────────────────────────────┐
  │ Req 1: [██][██][██]                                   │
  │ Req 2: [██][██][██][██][██][██]                       │
  │ Req 3: [██]                                           │
  │ Free:  [░░][░░][░░][░░][░░]                          │
  └──────────────────────────────────────────────────────┘
  Pages allocated on demand, freed when done
  Waste: <4% (only last page of each request)
  → Fit 2-4x more concurrent requests → 2-4x throughput
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    vLLM Engine                           │
│                                                          │
│  ┌──────────────────┐   ┌────────────────────────────┐ │
│  │  Scheduler        │   │  KV Cache Manager           │ │
│  │  (continuous      │   │  (PagedAttention)           │ │
│  │   batching)       │   │  ┌──────┬──────┬──────┐   │ │
│  │                   │   │  │Page 0│Page 1│Page 2│...│ │
│  │  Priority queue   │   │  └──────┴──────┴──────┘   │ │
│  │  of requests      │   │  Block table per request    │ │
│  └────────┬──────────┘   └────────────┬───────────────┘ │
│           │                           │                  │
│  ┌────────▼───────────────────────────▼──────────────┐ │
│  │                Model Execution                     │ │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐           │ │
│  │  │  GPU 0  │  │  GPU 1  │  │  GPU 2  │  ...      │ │
│  │  │ (tensor │  │ (tensor │  │ (tensor │           │ │
│  │  │  parallel│  │  parallel│  │  parallel│           │ │
│  │  └─────────┘  └─────────┘  └─────────┘           │ │
│  └───────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Key Innovations

### 1. PagedAttention

```
Traditional: contiguous KV cache per sequence
  Sequence "Hello world" with max_len=2048:
  ┌──────────────────────────────────────────────┐
  │ Hello │ world │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
  └──────────────────────────────────────────────┘
  Must pre-allocate full 2048 tokens. Can't share unused space.

PagedAttention: KV cache in pages (like OS virtual memory)
  Block table:  seq → [page 5, page 12, page 3]
  ┌────────┐         ┌────────┐
  │ Page 5 │ Hello   │ Page 12│ world
  └────────┘         └────────┘
  Pages allocated on demand. Free pages returned to pool.

  Bonus: copy-on-write for beam search
  Parallel beams share prefix pages, only copy when they diverge.
```

### 2. Continuous Batching

```
Static batching (naive):
  Wait for batch of 8 requests → process all → return all
  Problem: short request finishes, waits for longest request

  Time ─────────────────────────────►
  Req 1: [████████████████████████]  (long)
  Req 2: [████████]                  (short, done early)
  Req 3: [████████████]              (medium)
  GPU idle: ░░░░░░░░░░░░░░           (wasted!)

Continuous batching (vLLM):
  As soon as a request finishes → immediately add a new one
  GPU is ALWAYS processing a full batch

  Time ─────────────────────────────►
  Req 1: [████████████████████████]
  Req 2: [████████][Req 4: ████████████████]
  Req 3: [████████████][Req 5: ████████████]
  GPU utilization: ~100%
```

### 3. Tensor Parallelism (Multi-GPU)

```
Model too large for 1 GPU → split across GPUs

Tensor Parallel (TP=4):
  Each attention head / MLP column on a different GPU
  All GPUs process the SAME token, different parts of the model
  AllReduce to combine results after each layer

Pipeline Parallel (PP=2):
  First half of layers on GPU group A
  Second half on GPU group B
  Micro-batching to keep both groups busy

LLaMA-70B serving:
  TP=4 on 4×A100 → each GPU holds ~17.5B params → fits in 80GB
```

## Performance Numbers

| Configuration | Throughput (tokens/s) |
|--------------|----------------------|
| LLaMA-13B, 1×A100, naive | ~800 |
| LLaMA-13B, 1×A100, vLLM | ~2500 (3x faster) |
| LLaMA-70B, 4×A100, vLLM | ~1200 |
| LLaMA-70B, 8×H100, vLLM | ~4000 |

## vLLM vs Alternatives

| Feature | vLLM | TGI (HuggingFace) | Triton | TensorRT-LLM |
|---------|------|-------------------|--------|--------------|
| PagedAttention | ✅ Yes | ✅ Yes | ❌ No | ✅ Yes |
| Continuous batching | ✅ Yes | ✅ Yes | ❌ Manual | ✅ Yes |
| Ease of use | ✅ Simple | ✅ Simple | ❌ Complex | ❌ Complex |
| Custom models | ✅ Easy | ⚠️ Limited | ✅ Yes | ❌ Hard |
| Raw speed | Fast | Fast | Fastest | Fastest |
| Open source | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |

## Interview Sound Bite

> "For serving the LLM, I'd use vLLM because its PagedAttention eliminates KV cache memory waste — instead of pre-allocating max_context_length per request, it allocates in small pages on demand. This lets us serve 2-4x more concurrent requests. Combined with continuous batching, the GPU stays at near-100% utilization. For a 70B model, we'd use tensor parallelism across 4 A100s."
