# DeepSpeed — How ZeRO Eliminates Memory Redundancy

## What It Is

DeepSpeed (Microsoft) is a distributed training library. Its core innovation is **ZeRO** (Zero Redundancy Optimizer) — a technique to eliminate redundant memory usage across GPUs.

## The Memory Problem

```
Training a 7B model with Adam optimizer (standard DDP):

  Per parameter: 2B (fp16 param) + 2B (fp16 grad) + 4B (fp32 master weight)
                 + 4B (Adam m) + 4B (Adam v) = 16 bytes

  7B params × 16 bytes = 112 GB per GPU

  With DDP: EVERY GPU stores all 112 GB. Pure waste!
  8 GPUs × 112 GB = 896 GB total, but we only need 112 GB of unique data.

  ZeRO insight: shard across GPUs instead of duplicating.
```

## ZeRO Stages — Progressive Sharding

```
                Memory per GPU (7B model, 8 GPUs)
                ──────────────────────────────────
DDP (baseline): 112 GB per GPU × 8 = 896 GB total
                [param 112 | grad 14 | opt 84] all on every GPU

ZeRO Stage 1:  ~41 GB per GPU
  Shard: optimizer states (m, v, master weights)
  Each GPU: full params + full grads + 1/8 of optimizer
  Communication: AllGather optimizer state for param update
  [param 14 | grad 14 | opt 84/8=10.5]

ZeRO Stage 2:  ~27 GB per GPU
  Shard: optimizer states + gradients
  Each GPU: full params + 1/8 of grads + 1/8 of optimizer
  Communication: ReduceScatter gradients (not AllReduce)
  [param 14 | grad 14/8=1.75 | opt 10.5]

ZeRO Stage 3:  ~14 GB per GPU (= FSDP)
  Shard: optimizer states + gradients + parameters
  Each GPU: 1/8 of everything
  Communication: AllGather params before each layer, discard after
  [param 14/8=1.75 | grad 1.75 | opt 10.5]

ZeRO-Infinity:  fits on ANY hardware
  Offload to CPU RAM and NVMe SSD
  GPU only holds the active layer during computation
  [active layer only ~1-2 GB on GPU]
  10x-100x slower, but can train 1T params on a single node
```

## How ZeRO-3 Works Step by Step

```
8 GPUs training a model with 80 layers.
Each GPU holds 1/8 of each layer's parameters.

Forward pass for layer 5:
  1. AllGather: each GPU broadcasts its 1/8 of layer 5's weights
     → every GPU now has the FULL layer 5 weights (temporarily)
  2. Compute: each GPU runs forward on its data shard
  3. Discard: free the gathered weights (only keep own 1/8)
  4. Move to layer 6, repeat

Backward pass for layer 5:
  1. AllGather: gather full layer 5 weights again (needed for gradient computation)
  2. Compute gradients for layer 5
  3. ReduceScatter: each GPU sends its gradients, receives 1/8 of the sum
  4. Discard gathered weights, keep only gradient shard
  5. Update 1/8 of parameters with 1/8 of gradients

Communication per layer: 2 AllGathers (forward + backward) + 1 ReduceScatter
This is the overhead of ZeRO-3 vs DDP: ~10-20% slower per step,
but you can train models 8x larger.
```

## DeepSpeed vs PyTorch FSDP

```
Feature              DeepSpeed ZeRO-3      PyTorch FSDP
───────────────────────────────────────────────────────
Maintainer           Microsoft             PyTorch (Meta)
Integration          DeepSpeed launcher    native torch.distributed
ZeRO-1,2,3          yes                   FSDP ≈ ZeRO-3 only
CPU offload          ZeRO-Infinity         yes (basic)
NVMe offload         yes                   no
Mixed precision      yes                   yes
Activation checkpt   yes                   yes
Ease of use          config file           Python API
Composability        standalone            composes with DDP

Recommendation:
  - New projects: start with FSDP (native, simpler)
  - Need ZeRO-Infinity / advanced offload: use DeepSpeed
  - Already using DeepSpeed: keep using it
```

## DeepSpeed Inference

```
DeepSpeed also has an inference engine (separate from training):
  - Tensor parallelism for serving large models
  - Kernel fusion (fuse multiple ops into one GPU kernel)
  - Quantization (INT8, INT4)
  - MoE inference support

But for LLM serving, vLLM/SGLang are usually preferred due to
PagedAttention and continuous batching.
```
