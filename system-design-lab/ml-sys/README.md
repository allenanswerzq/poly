# ML Systems — The Infrastructure Behind Modern AI

Everything between the math and production. The systems that make training and serving LLMs actually work at scale.

## Table of Contents

1. [Training Systems](#1-training-systems)
2. [Parallelism Strategies](#2-parallelism-strategies)
3. [Communication Backends](#3-communication-backends)
4. [Serving Systems](#4-serving-systems)
5. [Memory Management](#5-memory-management)
6. [Data Pipeline](#6-data-pipeline)
7. [Orchestration](#7-orchestration)
8. [Key Papers & Systems](#8-key-papers--systems)

---

## 1. Training Systems

### The Training Stack

```
Your training script (Python)
    │
    ▼
Framework (PyTorch / JAX)
    │    - Autograd, tensor ops, module abstractions
    ▼
Distributed Training Library (FSDP / DeepSpeed / Megatron)
    │    - Parallelism, communication, memory optimization
    ▼
Communication Backend (NCCL / Gloo)
    │    - GPU-to-GPU data transfer (AllReduce, AllGather)
    ▼
Hardware (GPU cluster + NVLink + InfiniBand)
```

### PyTorch Distributed Landscape

```
                        ┌───────────────────────────────────┐
                        │          Your Training Code        │
                        └───────────────┬───────────────────┘
                                        │
              ┌─────────────┬───────────┼───────────┬──────────────┐
              ▼             ▼           ▼           ▼              ▼
         PyTorch DDP    PyTorch FSDP  DeepSpeed   Megatron-LM   FairScale
         (data par.)   (fully sharded) (ZeRO)    (model par.)  (deprecated)
              │             │           │           │
              └─────────────┴───────────┴───────────┘
                                │
                          torch.distributed
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
                  NCCL                    Gloo
              (GPU comms)            (CPU comms)
                    │                       │
              NVLink / IB              Ethernet
```

### DDP vs FSDP vs DeepSpeed — When to Use What

```
Model fits on 1 GPU?
  │
  ├── YES → DDP (simplest, fastest per-step)
  │         Each GPU has a FULL model copy.
  │         Only gradients are communicated (AllReduce).
  │         Overhead: ~5% for gradient sync.
  │
  └── NO → Model too large for 1 GPU
            │
            ├── FSDP (PyTorch native, recommended for most cases)
            │   Shard weights + gradients + optimizer across GPUs.
            │   AllGather weights before each layer, discard after.
            │   Memory: ~1/N per GPU (N = world_size).
            │
            ├── DeepSpeed ZeRO (Microsoft, most flexible)
            │   ZeRO-1: shard optimizer states only
            │   ZeRO-2: shard optimizer + gradients
            │   ZeRO-3: shard optimizer + gradients + parameters (= FSDP)
            │   ZeRO-Infinity: offload to CPU RAM + NVMe SSD
            │
            └── Megatron-LM (NVIDIA, for the largest models)
                Tensor parallel: split individual layers across GPUs
                Pipeline parallel: split model layers into stages
                Sequence parallel: split along sequence dimension
                Used for: GPT-3, Llama training at NVIDIA
```

## 2. Parallelism Strategies

### Data Parallelism (DP)

```
The simplest form. Each GPU has a FULL copy of the model.
Data is split across GPUs. Gradients are averaged after each step.

  GPU 0: full model, batch[0:B/4]   → grad_0
  GPU 1: full model, batch[B/4:B/2] → grad_1
  GPU 2: full model, batch[B/2:3B/4]→ grad_2
  GPU 3: full model, batch[3B/4:B]  → grad_3
                                        │
                                  AllReduce(avg)
                                        │
                                  avg_grad → all GPUs update identically

Scaling efficiency: 4 GPUs ≈ 3.5x speedup (communication overhead)
Memory: each GPU needs enough for full model + optimizer + activations
Limitation: model must fit on 1 GPU
```

### Tensor Parallelism (TP)

```
Split individual LAYERS across GPUs. Each GPU computes part of each layer.

For a linear layer W (4096 × 4096), with TP=4:
  GPU 0: W[:, 0:1024]      → partial output, columns 0-1023
  GPU 1: W[:, 1024:2048]   → partial output, columns 1024-2047
  GPU 2: W[:, 2048:3072]   → partial output, columns 2048-3071
  GPU 3: W[:, 3072:4096]   → partial output, columns 3072-4095
                                │
                          AllReduce  (combine partial outputs)

Every layer requires an AllReduce → needs FAST interconnect (NVLink).
Only practical WITHIN a node (8 GPUs connected by NVLink at 900 GB/s).
Across nodes (InfiniBand at 50 GB/s) → too slow for per-layer sync.
```

### Pipeline Parallelism (PP)

```
Split model into STAGES. Each stage on a different GPU (group).

  Stage 0 (GPU 0-1): layers 0-15    → forward → send activations →
  Stage 1 (GPU 2-3): layers 16-31   → forward → send activations →
  Stage 2 (GPU 4-5): layers 32-47   → forward → send activations →
  Stage 3 (GPU 6-7): layers 48-63   → forward → compute loss

Problem: PIPELINE BUBBLES. While Stage 0 computes, Stages 1-3 are idle.

Solution: micro-batching. Split the batch into micro-batches.
While Stage 0 processes micro-batch 2, Stage 1 processes micro-batch 1.

  Time →
  Stage 0: [mb0][mb1][mb2][mb3]        [bw3][bw2][bw1][bw0]
  Stage 1:      [mb0][mb1][mb2][mb3]   [bw3][bw2][bw1][bw0]
  Stage 2:           [mb0][mb1][mb2][mb3][bw3][bw2][bw1][bw0]
  Stage 3:                [mb0][mb1][mb2][mb3][bw3][bw2][bw1][bw0]

  Bubble ratio ≈ (num_stages - 1) / num_microbatches
  With 4 stages, 16 micro-batches: bubble = 3/16 = 19% waste
```

### 3D Parallelism (the real answer for LLMs)

```
Combine ALL three for the largest models:

  TP=8 (within node: split layers across 8 GPUs)
  PP=4 (across nodes: 4 pipeline stages)
  DP=16 (replicate across 16 data-parallel groups)

  Total GPUs = TP × PP × DP = 8 × 4 × 16 = 512 GPUs

  Each "slot":
    8 GPUs within a node handle tensor parallelism (NVLink)
    4 nodes form a pipeline (InfiniBand)
    16 such pipelines process different data (AllReduce across pipelines)
```

### Sequence Parallelism (SP)

```
For very long sequences (100K+ tokens):
  Split the sequence across GPUs.
  Each GPU processes a chunk of the sequence.
  Attention: ring attention — pass KV blocks around a ring of GPUs.

  GPU 0: tokens 0-25000
  GPU 1: tokens 25001-50000
  GPU 2: tokens 50001-75000
  GPU 3: tokens 75001-100000

  Used in: long-context LLMs (GPT-4, Claude's 200K context)
```

## 3. Communication Backends

### NCCL (NVIDIA Collective Communications Library)

```
The standard for GPU-to-GPU communication. Optimized for:
  - NVLink (within node): 900 GB/s
  - InfiniBand (across nodes): 400 Gbps
  - PCIe (fallback): 32 GB/s

Key operations:
  AllReduce:   avg(data) → every GPU gets the average
               Used for: gradient averaging in DDP
  AllGather:   gather all shards → every GPU gets all data
               Used for: FSDP weight gathering before forward
  ReduceScatter: reduce + scatter in one op
               Used for: FSDP gradient reduction
  Broadcast:   one GPU's data → all GPUs
               Used for: weight initialization
```

### Ring AllReduce (how it works)

```
N GPUs, each with a gradient vector of size M.
Instead of: everyone sends to GPU 0 (bottleneck) → GPU 0 averages → broadcast

Ring: each GPU sends 1/N of its data to the next GPU in a ring.
After N-1 steps, every GPU has the full average.

  Step 1: GPU 0 → GPU 1 → GPU 2 → GPU 3 → GPU 0  (send chunk 0)
  Step 2: rotate which chunk each GPU sends
  ...
  After N-1 steps: done!

  Time = 2 × (N-1)/N × M / bandwidth ≈ 2 × M / bandwidth  (for large N)
  → Independent of N! Scales linearly with model size, not GPU count.
```

## 4. Serving Systems

### The Inference Stack

```
Client request ("translate this")
    │
    ▼
API Gateway / Load Balancer
    │
    ▼
Serving Framework (vLLM / TGI / Triton / SGLang)
    │    - Request scheduling, batching
    │    - KV cache management
    │    - Token streaming
    ▼
Model Runtime (PyTorch / TensorRT / ONNX)
    │    - Actual GPU computation
    ▼
GPU Hardware (A100 / H100)
```

### Key Serving Optimizations

```
1. Continuous Batching
   Don't wait for all requests to finish. As one completes, add a new one.
   GPU utilization: 30% (static) → 95% (continuous).

2. PagedAttention (vLLM)
   KV cache allocated in pages (like OS virtual memory).
   No wasted pre-allocated memory. 2-4x more concurrent requests.

3. Speculative Decoding
   Small model drafts N tokens. Large model verifies in 1 forward pass.
   2-3x faster generation. No quality loss (same distribution).

4. Quantization
   FP16 → INT8: 2x less memory, ~2x faster, <1% quality loss
   FP16 → INT4 (GPTQ/AWQ): 4x less memory, ~3x faster, ~2% quality loss

5. Prefix Caching (SGLang RadixAttention)
   System prompt KV cache shared across requests.
   "You are a helpful assistant" computed ONCE, reused for all users.

6. Flash Attention
   IO-aware attention: minimize GPU memory reads/writes.
   2x faster, O(N) memory instead of O(N²).

7. KV Cache Compression
   Sliding window: only keep last W tokens (Mistral, window=4096)
   GQA: fewer KV heads (LLaMA 2: 8 KV heads vs 32 Q heads → 4x less cache)
```

### Serving System Comparison

```
                  vLLM      SGLang     TGI        TensorRT-LLM   Triton
PagedAttention     yes       yes       yes         yes            no
Cont. batching     yes       yes       yes         yes            manual
Prefix caching     basic     radix     no          yes            no
Constrained gen    basic     FSM       basic       no             no
Multi-turn opt     no        yes       no          no             no
Ease of use        easy      easy      easy        hard           hard
Best for           batch     agents    serving     max perf       multi-model
                   serving   multi-    HuggingFace NVIDIA         ensemble
                             step      models      hardware
```

## 5. Memory Management

### GPU Memory Budget (training a 70B LLM)

```
For each parameter (in mixed precision training):

  Parameter (fp16):              2 bytes
  Gradient (fp16):               2 bytes
  Optimizer state (fp32):
    Adam momentum (m):           4 bytes
    Adam variance (v):           4 bytes
    Master weight (fp32 copy):   4 bytes
  ─────────────────────────────────────
  Total per parameter:           16 bytes

  70B parameters × 16 bytes = 1,120 GB  (just for model state!)
  + Activations (batch × seq_len × layers × hidden)

  Single A100 80GB: need at least 14 GPUs just for model state
  With FSDP sharding: each GPU holds 1,120GB / 14 = 80GB ≈ fits!
```

### Activation Memory

```
Forward pass stores intermediate results for backward pass.
Each transformer layer stores:

  Input activation:      batch × seq_len × hidden      (e.g., 32 × 2048 × 4096 = 256M elements)
  Attention scores:      batch × heads × seq² / 2      (if using Flash Attention: much less)
  MLP intermediate:      batch × seq_len × 4*hidden    (the 4x expansion)

Total activations ≈ batch_size × seq_len × hidden × num_layers × ~12 bytes

Reducing activation memory:
  1. Gradient checkpointing: don't store, recompute during backward (~30% slower)
  2. Flash Attention: O(N) instead of O(N²) for attention activations
  3. Smaller batch size (but then need gradient accumulation)
```

### Memory vs Compute Tradeoffs

```
Technique            Memory saved    Compute cost    When to use
────────────────────────────────────────────────────────────────
Mixed precision      ~2x             ~0%             Always
Gradient checkpoint  ~50%            ~30%            Large models
Flash Attention      ~10-50%         ~0% (faster!)   Always
FSDP (ZeRO-3)      ~Nx (N=GPUs)    ~10-20%         Model > 1 GPU
Activation offload   ~80%            ~50%            Extreme cases
CPU offload          ~variable       ~100%+          Very large models, small GPU
```

## 6. Data Pipeline

### Training Data Flow

```
Raw data (Common Crawl, 100TB)
    │
    ▼
Cleaning / Dedup (deduplicate, filter, quality score)
    │
    ▼
Tokenization (BPE → token IDs, stored as binary)
    │
    ▼
Shuffling + Sharding (split into files, shuffle across epochs)
    │
    ▼
DataLoader (read shards, batch, pad/pack sequences)
    │
    ▼
GPU (transfer batch from CPU RAM → GPU HBM via PCIe/NVLink)

Key insight: data pipeline must be FASTER than GPU training step.
If data loading is the bottleneck → GPU sits idle → wasted $$$.

Solutions:
  - Pre-tokenize and store as binary (no tokenization at train time)
  - Multiprocess data loading (num_workers=8+)
  - Prefetch next batch while GPU processes current batch
  - Memory-mapped files (mmap) to avoid loading entire dataset to RAM
```

### Sequence Packing

```
Without packing (padding waste):
  [The cat sat <pad> <pad> <pad> <pad> <pad>]  ← 62% of compute wasted on padding!
  [Hello world <pad> <pad> <pad> <pad> <pad>]
  [This is a longer sentence with more <pad>]

With packing (no waste):
  [The cat sat <sep> Hello world <sep> Short]   ← 100% of compute useful
  [This is a longer sentence with more words]

  Attention mask prevents cross-contamination between packed sequences.
  2-3x more efficient for datasets with variable-length sequences.
```

## 7. Orchestration

### Training Job Lifecycle

```
1. Developer writes training config (model, data, hyperparams, GPU count)
2. Scheduler allocates GPU nodes (K8s + GPU scheduler / Slurm)
3. Launcher starts processes (torchrun / deepspeed launcher)
4. Each process:
   - Initializes NCCL communication (discover peers, create rings)
   - Loads its shard of the model + optimizer
   - Loads its shard of the data
   - Training loop:
     for step in steps:
       batch = next(dataloader)
       loss = model(batch)
       loss.backward()            ← NCCL gradient sync happens here
       optimizer.step()
       if step % N == 0: save checkpoint
5. Save final model to model registry
```

### Failure Recovery

```
GPU failures are COMMON at scale (1000+ GPUs):
  - GPU memory errors (ECC)
  - Network failures (InfiniBand link down)
  - Node crashes (kernel panic, OOM kill)

Probability of failure per node per day: ~1-5%
1024 GPUs = 128 nodes → expect ~2-6 failures per day!

Recovery strategy:
  1. Checkpoint every N steps (e.g., every 1000 steps)
  2. On failure: kill all processes
  3. Replace failed node (or use spare)
  4. Restart all processes from last checkpoint
  5. Lost work: at most N steps (~30 minutes if N=1000)

Elastic training (newer):
  - Detect failure → remove dead node → continue with fewer GPUs
  - No restart needed, but requires re-sharding model state
  - PyTorch TorchElastic, DeepSpeed Elastic
```

## 8. Key Papers & Systems

### Must-Know Systems (in chronological order)

| Year | System | Key Innovation | Impact |
|------|--------|---------------|--------|
| 2017 | Transformer | Self-attention, no recurrence | Everything since |
| 2018 | GPipe | Pipeline parallelism with micro-batches | Train very deep models |
| 2019 | Megatron-LM | Tensor + Pipeline + Data parallelism | Train GPT-scale models |
| 2020 | ZeRO (DeepSpeed) | Shard optimizer/gradient/params | 8x memory reduction |
| 2020 | GShard | Mixture of Experts at scale | Efficient scaling |
| 2022 | Flash Attention | IO-aware tiled attention | 2x faster, O(N) memory |
| 2022 | ALiBi / RoPE | Length extrapolation | Long-context models |
| 2023 | vLLM + PagedAttention | OS-style KV cache management | 2-4x serving throughput |
| 2023 | QLoRA | 4-bit quantized LoRA fine-tuning | Fine-tune 65B on 1 GPU |
| 2023 | SGLang + RadixAttention | Prefix-aware LLM serving | 3-5x for multi-turn |
| 2024 | Flash Attention 3 | Async + warp specialization on H100 | 1.5x over FA2 |
| 2024 | Ring Attention | Sequence parallelism for long context | 1M+ token context |
| 2024 | RLHF / DPO / GRPO | Alignment training | Safe, helpful models |

### Reading List for Interviews

**Must read:**
- Attention Is All You Need (2017) — the Transformer
- Megatron-LM (2020) — 3D parallelism
- ZeRO (2020) — memory-efficient distributed training
- Flash Attention (2022) — IO-aware attention
- vLLM / PagedAttention (2023) — efficient serving

**Good to know:**
- GPipe (2019) — pipeline parallelism
- LoRA (2021) — parameter-efficient fine-tuning
- RLHF / InstructGPT (2022) — alignment
- Chinchilla (2022) — optimal compute/data scaling laws
- SGLang (2024) — structured generation serving
