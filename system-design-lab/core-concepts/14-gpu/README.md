# GPU Architecture — What Principal Engineers Must Know

## Why This Matters

GPUs are the engine behind ML training, inference, and increasingly general-purpose computing. As a principal engineer, you need to understand why GPUs are fast for some workloads and terrible for others, how to reason about GPU utilization, and the programming model.

## 1. CPU vs GPU — Fundamentally Different

```
CPU: few cores, each very fast          GPU: thousands of cores, each simple
┌─────────────────────┐                ┌─────────────────────────────────┐
│ Core 0  Core 1      │                │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ ┌─────┐ ┌─────┐     │                │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ │█████│ │█████│     │                │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ │█████│ │█████│     │                │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ │█████│ │█████│     │                │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ └─────┘ └─────┘     │                │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ █ = control logic    │                │ ░ = tiny compute core           │
│ (branch predict,     │                │ (no branch prediction,          │
│  out-of-order, cache)│                │  in-order, tiny cache)          │
└─────────────────────┘                └─────────────────────────────────┘

CPU: optimize for LATENCY (1 task fast)
GPU: optimize for THROUGHPUT (10000 tasks parallel)
```

| | CPU | GPU |
|---|---|---|
| Cores | 8-96 (complex) | 10,000+ (simple) |
| Clock | 3-5 GHz | 1.5-2.5 GHz |
| Cache | 30-384 MB L3 | 50-60 MB L2 |
| Memory BW | 100-460 GB/s | 2,000-3,350 GB/s |
| Best for | Sequential, branchy code | Massive parallel, regular computation |

## 2. GPU Architecture (NVIDIA)

```
┌─────────────────────────────────────────────────────────────────┐
│                     GPU (e.g., A100)                              │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                  GPC (Graphics Processing Cluster)       │    │
│  │                                                          │    │
│  │  ┌───────────────────────────────────────────────┐      │    │
│  │  │         SM (Streaming Multiprocessor)          │      │    │
│  │  │                                                │      │    │
│  │  │  ┌────────────────────────────────────────┐   │      │    │
│  │  │  │ CUDA Cores: 64 FP32 + 32 FP64          │   │      │    │
│  │  │  │ Tensor Cores: 4 (matrix multiply units) │   │      │    │
│  │  │  │ Register File: 256 KB                   │   │      │    │
│  │  │  │ Shared Memory / L1: 192 KB              │   │      │    │
│  │  │  │ Warp Scheduler: 4 (each handles 32 thr) │   │      │    │
│  │  │  └────────────────────────────────────────┘   │      │    │
│  │  │  × 16 SMs per GPC                             │      │    │
│  │  └───────────────────────────────────────────────┘      │    │
│  │  × 8 GPCs                                                │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Total: 108 SMs, 6912 CUDA cores, 432 Tensor Cores (A100)      │
│                                                                  │
│  ┌──────────────┐                                               │
│  │ L2 Cache     │  40 MB (A100) / 50 MB (H100)                 │
│  └──────┬───────┘                                               │
│         │                                                        │
│  ┌──────▼───────┐                                               │
│  │ HBM Memory   │  80 GB @ 2 TB/s (A100) / 80GB @ 3.35 TB/s   │
│  └──────────────┘                                               │
└─────────────────────────────────────────────────────────────────┘
```

### Key Building Blocks

**SM (Streaming Multiprocessor)**: the GPU's "core" — contains CUDA cores, tensor cores, registers, shared memory. Each SM runs multiple warps concurrently.

**Warp**: 32 threads that execute the SAME instruction at the SAME time (SIMT — Single Instruction Multiple Thread).

**Tensor Core**: specialized matrix multiply unit. Does a 4×4 matrix multiply in 1 cycle (vs hundreds of cycles on CUDA cores).

## 3. Programming Model (CUDA Hierarchy)

```
┌─────────────────────────────────────────────────────────────┐
│                    CUDA Programming Model                    │
│                                                              │
│  Grid (entire kernel launch)                                 │
│  ┌───────────────────────────────────────────────────┐      │
│  │ Block (0,0)    Block (1,0)    Block (2,0)          │      │
│  │ ┌───────────┐ ┌───────────┐  ┌───────────┐        │      │
│  │ │ Thread 0  │ │ Thread 0  │  │ Thread 0  │        │      │
│  │ │ Thread 1  │ │ Thread 1  │  │ Thread 1  │        │      │
│  │ │ ...       │ │ ...       │  │ ...       │        │      │
│  │ │ Thread 255│ │ Thread 255│  │ Thread 255│        │      │
│  │ └───────────┘ └───────────┘  └───────────┘        │      │
│  │                                                    │      │
│  │ Block (0,1)    Block (1,1)    Block (2,1)          │      │
│  │ ┌───────────┐ ┌───────────┐  ┌───────────┐        │      │
│  │ │ 256 threads│ │ 256 threads│ │ 256 threads│       │      │
│  │ └───────────┘ └───────────┘  └───────────┘        │      │
│  └───────────────────────────────────────────────────┘      │
│                                                              │
│  Grid   → maps to entire GPU                                │
│  Block  → maps to 1 SM (shared memory visible within block) │
│  Warp   → 32 threads executing same instruction (SIMT)      │
│  Thread → individual lane                                    │
└─────────────────────────────────────────────────────────────┘
```

### Memory Hierarchy
```
                            Bandwidth        Size        Scope
  ┌──────────────────┐
  │ Registers         │     ~20 TB/s        256KB/SM    per thread
  ├──────────────────┤
  │ Shared Memory     │     ~15 TB/s        192KB/SM    per block
  │ (L1 cache)        │                                 (programmer-managed)
  ├──────────────────┤
  │ L2 Cache          │     ~5 TB/s         40-50MB     all SMs
  ├──────────────────┤
  │ Global Memory     │     2-3.35 TB/s     80GB        all threads
  │ (HBM)             │                     (GPU DRAM)
  ├──────────────────┤
  │ Host Memory       │     ~32 GB/s        system RAM  CPU ↔ GPU (PCIe)
  │ (CPU RAM)         │     (PCIe 4.0)
  └──────────────────┘

Key insight:
  Registers → Shared Memory → L2 → HBM → CPU RAM
  Each level: ~5-10x slower than the one above

  HBM bandwidth (2 TB/s) is the #1 bottleneck for LLMs
  → Most of inference is MEMORY-BOUND, not compute-bound
```

## 4. What Makes Code GPU-Friendly?

```
GPU-friendly (fast):                 GPU-unfriendly (slow):
  ✓ Massively parallel               ✗ Sequential dependencies
  ✓ Same operation on all data        ✗ Different branches per thread
  ✓ Regular memory access             ✗ Random memory access
  ✓ High arithmetic intensity         ✗ Memory-bound with low reuse
  ✓ Large batches                     ✗ Small batches (launch overhead)

Examples:
  FAST: matrix multiply (parallel, regular, high compute)
  FAST: element-wise operations (add, relu, scale)
  FAST: convolution (regular pattern, high compute)
  SLOW: graph traversal (irregular, branchy)
  SLOW: linked list walking (pointer chasing)
  SLOW: sorting (lots of branching + data movement)
```

### Warp Divergence — Why Branches Kill GPU Performance
```
All 32 threads in a warp execute the SAME instruction.
If threads take different branches → serial execution:

  if (threadIdx.x < 16) {
      do_A();    // first 16 threads execute this, other 16 IDLE
  } else {
      do_B();    // next 16 threads execute this, first 16 IDLE
  }
  → 50% of warp is idle at any time. Half the performance!

  // Better: ensure all threads in a warp take same branch
  // Or: restructure to avoid branches entirely
```

## 5. Tensor Cores — Why GPUs Dominate ML

```
CUDA Core: 1 multiply-add per cycle per core
  C[i][j] += A[i][k] * B[k][j]    ← 1 operation

Tensor Core: 4×4 matrix multiply-add per cycle
  C[4×4] += A[4×4] × B[4×4]       ← 64 operations in 1 cycle!

                        CUDA Cores      Tensor Cores
A100 FP16 peak:         78 TFLOPS      312 TFLOPS      (4x faster)
H100 FP16 peak:         268 TFLOPS     990 TFLOPS      (3.7x faster)

This is why:
  - ML training uses Tensor Cores (matmul is 70%+ of training compute)
  - Mixed precision (FP16 compute, FP32 accumulate) = use Tensor Cores
  - torch.compile / cuDNN automatically use Tensor Cores for eligible ops
```

### FP8 / FP16 / BF16 / FP32 — Precision Formats
```
FP32: 1 sign + 8 exponent + 23 mantissa (standard float)
FP16: 1 sign + 5 exponent + 10 mantissa (half precision)
BF16: 1 sign + 8 exponent + 7 mantissa  (brain float — same range as FP32!)
FP8:  1 sign + 4 exponent + 3 mantissa  (H100+, 2x faster than FP16)

Training:  BF16 (same range as FP32, fewer precision bits — works fine)
Inference: FP8 or INT8 (quantized — 2x throughput on H100)

Why BF16 over FP16?
  FP16 range: ±65504        ← overflows easily during training (loss scaling needed)
  BF16 range: ±3.4×10^38   ← same as FP32, no overflow issues
```

## 6. GPU Interconnects

```
Within a node (8 GPUs):
  NVLink: 900 GB/s bidirectional (H100)
  NVSwitch: full mesh — any GPU talks to any GPU at full speed

  ┌─────┐ NVLink ┌─────┐ NVLink ┌─────┐ NVLink ┌─────┐
  │GPU 0│◄──────►│GPU 1│◄──────►│GPU 2│◄──────►│GPU 3│
  └──┬──┘        └──┬──┘        └──┬──┘        └──┬──┘
     │    NVSwitch (all-to-all)    │               │
  ┌──▼──┐        ┌──▼──┐        ┌──▼──┐        ┌──▼──┐
  │GPU 4│◄──────►│GPU 5│◄──────►│GPU 6│◄──────►│GPU 7│
  └─────┘        └─────┘        └─────┘        └─────┘

Between nodes:
  InfiniBand: 400 Gbps (50 GB/s) — 18x slower than NVLink
  → This is why multi-node training is communication-bound
  → Overlap communication with computation to hide latency
```

## 7. Key GPU Specs to Know

| GPU | CUDA Cores | Tensor Cores | Memory | Memory BW | FP16 Peak | NVLink |
|-----|-----------|-------------|--------|-----------|-----------|--------|
| V100 | 5120 | 640 | 32GB HBM2 | 900 GB/s | 125 TFLOPS | 300 GB/s |
| A100 | 6912 | 432 | 80GB HBM2e | 2 TB/s | 312 TFLOPS | 600 GB/s |
| H100 | 16896 | 528 | 80GB HBM3 | 3.35 TB/s | 990 TFLOPS | 900 GB/s |
| H200 | 16896 | 528 | 141GB HBM3e | 4.8 TB/s | 990 TFLOPS | 900 GB/s |
| B200 | ~18000 | ~600 | 192GB HBM3e | 8 TB/s | ~2500 TFLOPS | 1800 GB/s |

## 8. Bottleneck Analysis — Compute vs Memory Bound

```
Arithmetic Intensity = FLOPs / Bytes loaded from memory

Matrix multiply (2048×2048): ~2048 FLOPs per byte loaded
  → COMPUTE BOUND (GPU cores are the bottleneck)
  → Tensor Cores help a LOT

LLM inference (generating 1 token):
  Load all model weights (70B × 2 bytes = 140GB) for ~140B FLOPs
  = ~1 FLOP per byte loaded
  → MEMORY BOUND (HBM bandwidth is the bottleneck)
  → More memory bandwidth helps more than more compute
  → This is why H200 (4.8 TB/s) is better for inference than H100 (3.35 TB/s)

Roofline model:
  peak_throughput = min(peak_compute, peak_bandwidth × arithmetic_intensity)

  If your workload is below the "roofline", you're memory-bound.
  If you're at the roofline, you're compute-bound.
```

## Interview Quick Reference

| Question | Key Points |
|----------|-----------|
| "Why are GPUs fast for ML?" | Thousands of cores for parallel matmul + Tensor Cores (64 ops/cycle) + massive memory BW (2-8 TB/s) |
| "Compute vs memory bound?" | LLM inference = memory bound (load 140GB weights for each token). Training = compute bound (reuse weights across batch). |
| "Why is batch size important?" | Larger batch = more data reuse per weight load = higher arithmetic intensity = better GPU utilization |
| "NVLink vs InfiniBand?" | NVLink: 900GB/s within node (GPU↔GPU). InfiniBand: 50GB/s between nodes. 18x gap = multi-node is communication-bottlenecked. |
| "FP16 vs BF16?" | BF16 = same range as FP32 (no overflow during training). FP16 = more precision but narrower range (needs loss scaling). |
| "What's warp divergence?" | 32 threads in a warp take different branches → half idle → 50% performance loss. Avoid branches or ensure warp-uniform branching. |

## Further Reading

- Cornell GPU Architecture Course: https://cvw.cac.cornell.edu/gpu-architecture
