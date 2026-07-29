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

### 2.1 How the Computation Graph Is Built

```
Every torch.Tensor has two key fields:

  tensor.data       → the actual numbers (stored on CPU or GPU)
  tensor.grad_fn    → pointer to the Function node that CREATED this tensor
                      (None if the tensor was created by the user, not by an op)
  tensor.requires_grad → should autograd track this tensor?

When you do operations on tensors with requires_grad=True,
PyTorch RECORDS every operation into a DAG (directed acyclic graph):

  # Python code                    # What PyTorch builds internally
  w = torch.randn(3, 3,
        requires_grad=True)        → w.grad_fn = None  (leaf tensor)

  x = torch.randn(3, 1)           → x.grad_fn = None  (leaf, no grad needed)

  y = w @ x                       → y.grad_fn = MmBackward0
                                     MmBackward0.saved_tensors = (w, x)
                                     MmBackward0.next = [w.grad_accumulator]

  z = y.relu()                    → z.grad_fn = ReluBackward0
                                     ReluBackward0.saved_tensors = (y,)
                                     ReluBackward0.next = [MmBackward0]

  loss = z.sum()                  → loss.grad_fn = SumBackward0
                                     SumBackward0.next = [ReluBackward0]

The graph after forward:

  ┌──────────────┐
  │   loss       │  loss.grad_fn = SumBackward0
  │  (scalar)    │
  └──────┬───────┘
         │ .next
         ▼
  ┌──────────────┐
  │ SumBackward0 │  "I know how to differentiate sum()"
  └──────┬───────┘
         │ .next
         ▼
  ┌──────────────┐
  │ReluBackward0 │  "I know how to differentiate relu()"
  │              │   saved: (y,) ← needs y to know where relu was 0
  └──────┬───────┘
         │ .next
         ▼
  ┌──────────────┐
  │ MmBackward0  │  "I know how to differentiate matmul()"
  │              │   saved: (w, x) ← needs both inputs for d/dw and d/dx
  └──────┬───────┘
         │ .next
         ▼
  ┌──────────────┐
  │ w (leaf)     │  w.grad_fn = None (this is where gradients accumulate)
  │              │  w.grad = None (will be filled by backward)
  └──────────────┘

  Each grad_fn is a FUNCTION NODE that knows:
    1. How to compute the LOCAL gradient (Jacobian) for its operation
    2. Which tensors it saved for this computation
    3. Which grad_fn to pass the result to next (.next pointers)
```

### 2.2 How backward() Walks the Graph (Reverse-Mode AD)

```
loss.backward() triggers the engine:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  Step 1: Start at loss. grad_output = 1.0 (dloss/dloss = 1)   │
  │                                                                  │
  │  Step 2: SumBackward0.backward(grad_output=1.0)                │
  │    sum is: loss = z0 + z1 + z2                                  │
  │    ∂loss/∂zi = 1.0 for all i                                   │
  │    → passes [1.0, 1.0, 1.0] to ReluBackward0                  │
  │                                                                  │
  │  Step 3: ReluBackward0.backward(grad_output=[1, 1, 1])        │
  │    relu(y) = y if y > 0, else 0                                │
  │    ∂relu/∂y = 1 if y > 0, else 0                              │
  │    Uses saved tensor y to compute the mask:                    │
  │      y = [0.5, -0.3, 1.2]                                     │
  │      mask = [1, 0, 1]        (where y > 0)                    │
  │    grad = grad_output * mask = [1, 0, 1]                       │
  │    → passes [1, 0, 1] to MmBackward0                          │
  │                                                                  │
  │  Step 4: MmBackward0.backward(grad_output=[1, 0, 1])         │
  │    y = w @ x                                                    │
  │    ∂y/∂w = grad_output @ x.T     (gradient w.r.t. weights)    │
  │    ∂y/∂x = w.T @ grad_output     (gradient w.r.t. input)      │
  │    Uses saved tensors (w, x) for this computation.             │
  │                                                                  │
  │    → ∂y/∂w is ACCUMULATED into w.grad                         │
  │      (w.grad += ∂y/∂w, because w is a leaf tensor)            │
  │    → ∂y/∂x is discarded (x.requires_grad=False)              │
  │                                                                  │
  │  Step 5: Done. w.grad now contains ∂loss/∂w.                  │
  │    optimizer.step() uses w.grad to update w.                   │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

The chain rule in action:

  ∂loss/∂w = ∂loss/∂loss × ∂loss/∂z × ∂z/∂y × ∂y/∂w
               (=1)        (sum)     (relu)   (matmul)
               Step 1      Step 2    Step 3    Step 4

  Each grad_fn computes ONE local derivative and passes it backward.
  The chain rule is just multiplication along the path.
```

### 2.3 What "Saved Tensors" Means for Memory

```
During forward, each grad_fn SAVES tensors needed for backward:

  MmBackward0 saves (w, x)     → needs both to compute d/dw and d/dx
  ReluBackward0 saves (y,)     → needs y to know the relu mask
  SoftmaxBackward saves (out,) → needs softmax output for gradient

  These saved tensors stay in GPU memory until backward() runs.
  THIS is why activations dominate memory during training:

    Forward through 80 transformer layers:
      Each layer saves activations for backward.
      Hidden dim 8192, batch=32, seq=2048, bf16:
        Per layer: 32 × 2048 × 8192 × 2 bytes ≈ 1 GB
        80 layers: ~80 GB of saved activations.
        This is why large models OOM even if weights fit.

  Gradient checkpointing solves this:
    DON'T save activations. Instead, re-run forward during backward
    to recompute them. Trades ~33% extra compute for ~80% less memory.

    @torch.utils.checkpoint.checkpoint
    def forward_block(x):
        return transformer_layer(x)
    # Activations NOT saved. Recomputed during backward.
```

### 2.4 The Autograd Engine (C++)

```
loss.backward() calls the C++ autograd engine:

  1. Start with a queue: [(loss.grad_fn, grad_output=1.0)]

  2. TOPOLOGICAL SORT (reverse order):
     The engine processes nodes in reverse topological order
     so that every node's outputs are ready before it runs.
     (In practice, it uses a priority queue / task queue.)

  3. For each node in the queue:
     a) Call node.backward(grad_output) → computes local gradient
     b) For each input tensor that needs grad:
        If leaf tensor: ACCUMULATE grad into tensor.grad
        If intermediate: push (next_grad_fn, grad) into the queue

  4. Multi-threading:
     The engine uses a THREAD POOL (not one thread).
     Independent branches of the graph can run in parallel.
     But on GPU: actual backward kernels run on CUDA streams,
     so the C++ threads mostly just dispatch work to the GPU.

  5. After backward:
     - The computation graph is DESTROYED (freed).
       grad_fn pointers are cleared. Saved tensors are freed.
       This is why you can't call backward() twice by default.
       (Use retain_graph=True to keep it, but that leaks memory.)
     - Leaf tensors (parameters) have .grad populated.
     - Optimizer reads .grad and updates parameters.

  ┌──────────────────────────────────────────────────────────┐
  │                                                          │
  │  forward():                                             │
  │    builds graph (grad_fn chain + saved tensors)         │
  │    GPU memory grows (activations saved)                 │
  │                                                          │
  │  backward():                                            │
  │    walks graph in reverse                               │
  │    computes gradients via chain rule                    │
  │    frees saved tensors as it goes (memory shrinks)      │
  │    destroys the graph when done                         │
  │                                                          │
  │  optimizer.step():                                       │
  │    reads .grad, updates params                          │
  │    no graph involvement                                 │
  │                                                          │
  │  optimizer.zero_grad():                                  │
  │    resets .grad to None (or zero)                        │
  │    ready for next forward-backward cycle                │
  │                                                          │
  └──────────────────────────────────────────────────────────┘
```

### 2.5 no_grad, inference_mode, and detach

```
Three ways to STOP autograd from tracking:

  torch.no_grad():
    with torch.no_grad():
        y = model(x)
    # No grad_fn nodes created. No saved tensors. No graph.
    # y.requires_grad = False. Can't call y.backward().
    # Use for: inference, evaluation, manual weight updates.

  torch.inference_mode():
    with torch.inference_mode():
        y = model(x)
    # Like no_grad() but FASTER — skips more internal bookkeeping.
    # Tensors created here can't be used in autograd at all.
    # Use for: production inference (slightly faster than no_grad).

  tensor.detach():
    y = (w @ x).detach()
    # Creates a NEW tensor sharing the same data but with no grad_fn.
    # Breaks the graph at this point.
    # Use for: stopping gradient flow through specific paths.

  Why these matter for training:
    - Evaluation loop should ALWAYS use no_grad/inference_mode
      (otherwise you waste GPU memory building unused graphs)
    - Teacher model in knowledge distillation → detach()
    - GAN discriminator output when training generator → detach()
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
