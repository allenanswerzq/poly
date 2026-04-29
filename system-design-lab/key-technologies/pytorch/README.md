# PyTorch — Deep Learning Framework

## Overview

PyTorch is the **dominant framework for ML research and increasingly for production**. you need to understand its execution model, distributed training primitives, and how it maps to hardware.

## History & Why It Exists

```
The problem (2015-2016):
  TensorFlow (Google, 2015) dominated ML but had a painful API:
    - Define-then-run: build a static computation graph, THEN execute it.
    - Debugging was terrible (couldn't use Python debugger on the graph).
    - Dynamic models (variable-length sequences, tree structures) were hard.

  Researchers wanted something that felt like NumPy but ran on GPUs.
  "Just write Python. Use normal if/for/while. Debug with print()."

  Soumith Chintala and the Facebook AI Research (FAIR) team built PyTorch:
    - Define-by-run: computation graph built dynamically as you execute.
    - Feels like NumPy with GPU support + automatic differentiation.
    - Based on Torch (Lua framework), rewritten in Python.

Timeline:
  2002  Torch (Lua-based ML framework, NYU) — PyTorch's ancestor
  2016  PyTorch 0.1 released by Facebook AI Research
  2017  Rapid adoption by ML researchers (NeurIPS/ICML papers shift)
  2018  PyTorch 1.0 (TorchScript for production, C++ frontend)
  2020  PyTorch overtakes TensorFlow in research paper count
  2022  PyTorch 2.0 (torch.compile — the compiler revolution)
  2022  PyTorch Foundation moves to Linux Foundation (not just Meta)
  2023  PyTorch dominates both research AND production
  2024  PyTorch 2.4+ (torch.export, FlexAttention, compiled custom ops)

Why PyTorch won:
  1. EAGER MODE: run code line-by-line, debug with print(). Researchers loved it.
  2. PYTHONIC: feels like writing normal Python, not a DSL.
  3. Dynamic graphs: easily handle variable-length inputs (NLP, speech).
  4. Community: every paper releases PyTorch code. Ecosystem effect.
  5. torch.compile (2.0): closed the performance gap with TF/XLA.
     Now you get both ease-of-use AND compiled performance.

PyTorch vs TensorFlow:
  2016-2019: TF for production, PyTorch for research
  2020-2022: PyTorch dominates research, TF still in some production
  2023+:     PyTorch dominant everywhere. TF is maintenance mode.
             Google themselves use JAX internally now, not TF.
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│              PyTorch Execution Architecture                       │
│                                                                   │
│  y = model(x)  →  what happens?                                  │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                    Python Frontend                        │    │
│  │                                                           │    │
│  │  torch.nn.Module  ─►  forward() method                    │    │
│  │  torch.Tensor     ─►  data + grad + device info           │    │
│  │  torch.autograd   ─►  builds computation graph on-the-fly │    │
│  └──────────────────────────────────────────────────────────┘    │
│         │ calls C++ dispatcher                                    │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │               torch.dispatch  (C++ core)                  │    │
│  │                                                           │    │
│  │  Dispatcher routes operations based on:                   │    │
│  │    - Device (CPU / CUDA / MPS)                            │    │
│  │    - Dtype (float32 / bfloat16 / int8)                    │    │
│  │    - Autograd (record op for backward pass)               │    │
│  │    - Compile (TorchDynamo captures the graph)             │    │
│  └──────────────────────────────────────────────────────────┘    │
│         │ dispatches to backend                                    │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                  ATen (tensor library, C++)               │    │
│  │                                                           │    │
│  │  1800+ tensor operations: matmul, conv2d, relu, etc.     │    │
│  │  Each op has CPU, CUDA, and sometimes MPS implementations│    │
│  │                                                           │    │
│  │  CPU: calls MKL/oneDNN (Intel) or OpenBLAS               │    │
│  │  CUDA: calls cuBLAS (matmul), cuDNN (conv), custom kernels│   │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘

torch.compile PIPELINE (PyTorch 2.0+):
  ┌───────────────────────────────────────────────────────┐
  │  @torch.compile                                        │
  │  def f(x): return relu(x @ w + b)                      │
  │                                                        │
  │  Step 1: TorchDynamo                                   │
  │    Intercepts Python bytecode                           │
  │    Captures computation graph (FX Graph)                │
  │    Handles control flow, data-dependent shapes          │
  │       │                                                │
  │       ▼                                                │
  │  Step 2: AOTAutograd                                   │
  │    Traces forward + backward pass at compile time       │
  │    Produces joint graph for both directions             │
  │       │                                                │
  │       ▼                                                │
  │  Step 3: Inductor (default backend)                    │
  │    FX Graph → Triton kernels (GPU) or C++ (CPU)        │
  │    Fuses ops: matmul+add+relu → ONE kernel             │
  │       │                                                │
  │       ▼                                                │
  │  Step 4: Triton → PTX → GPU machine code               │
  │    Auto-tunes tile sizes per GPU model                  │
  │       │                                                │
  │       ▼                                                │
  │  Cached compiled kernel (fast on subsequent calls)     │
  └───────────────────────────────────────────────────────┘

DISTRIBUTED TRAINING (multi-GPU):
  ┌───────────────────────────────────────────────────────┐
  │  DDP (Distributed Data Parallel):                       │
  │                                                        │
  │  GPU 0        GPU 1        GPU 2        GPU 3         │
  │  ┌──────┐    ┌──────┐    ┌──────┐    ┌──────┐    │
  │  │ Full  │    │ Full  │    │ Full  │    │ Full  │    │
  │  │ Model │    │ Model │    │ Model │    │ Model │    │
  │  │ Copy  │    │ Copy  │    │ Copy  │    │ Copy  │    │
  │  └───┬──┘    └───┬──┘    └───┬──┘    └───┬──┘    │
  │      │            │            │            │         │
  │      └──── AllReduce (avg gradients) ────┘         │
  │              via NCCL (GPU-to-GPU, NVLink)              │
  │                                                        │
  │  Each GPU gets different data batch (data parallelism)  │
  │  Forward: independent. Backward: AllReduce gradients.   │
  │  All models stay synchronized.                          │
  │                                                        │
  │  FSDP: shards model across GPUs (for huge models).     │
  │  Each GPU holds 1/N of parameters. Gather before use.  │
  └───────────────────────────────────────────────────────┘
```

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
