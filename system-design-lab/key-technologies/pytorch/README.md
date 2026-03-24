# PyTorch — Deep Learning Framework

## Overview

PyTorch is the **dominant framework for ML research and increasingly for production**. As a principal engineer, you need to understand its execution model, distributed training primitives, and how it maps to hardware.

## Core Concepts

### 1. Tensors — The Fundamental Data Type

```python
# Tensor = multi-dimensional array on CPU or GPU
x = torch.randn(32, 3, 224, 224)  # batch of 32 images, 3 channels, 224×224
x = x.to("cuda:0")                 # move to GPU 0

# Operations happen on the device where the tensor lives
# GPU operations are ASYNCHRONOUS — they queue on the CUDA stream
y = model(x)      # queues GPU operations (returns immediately)
loss = criterion(y, labels)
loss.backward()   # queues backward pass on GPU
torch.cuda.synchronize()  # wait for all GPU ops to finish
```

### 2. Autograd — Automatic Differentiation

```
Forward pass: builds a computation graph
Backward pass: traverses graph in reverse to compute gradients

  x ──► linear ──► relu ──► linear ──► loss
                                         │
  backward(): loss → ∂loss/∂w₂ → ∂loss/∂relu → ∂loss/∂w₁
              walks the graph in reverse, computing gradients

This is why PyTorch is "define-by-run" (dynamic graph):
  The graph is built fresh each forward pass
  → conditional logic, variable-length inputs all work naturally
  (vs TensorFlow 1.x which built a static graph first)
```

### 3. GPU Memory — The #1 Practical Concern

```
GPU Memory budget (A100 80GB):
  Model parameters:     ~2 bytes per param (fp16)
  Gradients:            ~2 bytes per param
  Optimizer state:      ~8 bytes per param (Adam: m + v + master weights)
  Activations:          varies (proportional to batch size × model size)

70B parameter model:
  Parameters:    140 GB (fp16)  ← doesn't fit on 1 GPU!
  Gradients:     140 GB
  Optimizer:     560 GB
  Total:         ~840 GB → need at least 11 × A100 80GB

Techniques to reduce memory:
  Mixed precision (fp16/bf16):     halve parameter memory
  Gradient checkpointing:          trade compute for memory (recompute activations)
  ZeRO (DeepSpeed):                shard optimizer state across GPUs
  Tensor parallelism:              split individual layers across GPUs
  Pipeline parallelism:            split model layers across GPUs
```

### 4. Distributed Data Parallel (DDP)

```python
# The standard way to train on multiple GPUs
model = DistributedDataParallel(model, device_ids=[local_rank])

# What DDP does:
# 1. Each GPU has a FULL copy of the model
# 2. Each GPU processes a different mini-batch
# 3. After backward(), gradients are AllReduced (averaged)
# 4. All GPUs apply the same gradient update → stay in sync

# AllReduce communication:
#   Ring AllReduce: each GPU sends/receives to neighbors in a ring
#   Communication time ≈ 2 × model_size / bandwidth
#   With NVLink (600 GB/s): 70B model → ~0.5 seconds per step
#   With ethernet (100 Gbps): 70B model → ~11 seconds per step
```

### 5. FSDP — Fully Sharded Data Parallel

```
DDP: each GPU holds FULL model (wastes memory)
FSDP: shard model across GPUs (like DeepSpeed ZeRO-3)

DDP (4 GPUs):                    FSDP (4 GPUs):
  GPU 0: full model                GPU 0: 1/4 of params + 1/4 optimizer
  GPU 1: full model                GPU 1: 1/4 of params + 1/4 optimizer
  GPU 2: full model                GPU 2: 1/4 of params + 1/4 optimizer
  GPU 3: full model                GPU 3: 1/4 of params + 1/4 optimizer

FSDP: before each layer's forward, AllGather the full params
      after backward, reduce-scatter the gradients
      → more communication, but fits much larger models
```

## PyTorch Compilation Stack (torch.compile)

```
model = torch.compile(model)

What it does:
  1. Traces the Python code to capture the computation graph
  2. Optimizes the graph (fuse operations, eliminate overhead)
  3. Generates optimized GPU kernels (via Triton or CUDA)

Speedup: 30-70% on common models (less Python overhead, fused kernels)
```

## Key Numbers

| Metric | Value |
|--------|-------|
| A100 FP16 peak | 312 TFLOPS |
| A100 memory bandwidth | 2 TB/s |
| A100 NVLink bandwidth | 600 GB/s |
| H100 FP16 peak | 990 TFLOPS |
| H100 memory bandwidth | 3.35 TB/s |
| GPT-3 training cost | ~$4.6M (2020 prices) |
| LLaMA-70B training | ~$2M (2023) |

## Interview Sound Bite

> "For training a 70B model, we can't fit it on a single GPU — it needs ~840GB for params + optimizer. I'd use FSDP to shard across 8 A100s per node, with DDP across nodes. Each layer's parameters are gathered just-in-time for the forward pass and sharded again after. This trades communication for memory, letting us train models that are 8x larger than what DDP alone supports."
