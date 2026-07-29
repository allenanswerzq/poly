# GPU Architecture — What Must Know

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

### Full Chip View — From PCIe Slot to CUDA Core

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        NVIDIA A100 GPU — Full Chip Layout                     │
│                                                                               │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │                         PCIe / NVLink Interface                          │ │
│  │  PCIe Gen4 x16 (32 GB/s) for CPU ↔ GPU data transfers                  │ │
│  │  NVLink 3.0: 12 links × 50 GB/s = 600 GB/s total (GPU ↔ GPU)          │ │
│  │  NVSwitch: all-to-all connectivity for multi-GPU (DGX A100)            │ │
│  └────────────────────────────────┬────────────────────────────────────────┘ │
│                                   │                                          │
│  ┌────────────────────────────────▼────────────────────────────────────────┐ │
│  │                     GigaThread Engine                                    │ │
│  │  Top-level work distributor. Receives kernel launch from CPU driver.    │ │
│  │  Distributes thread blocks (CTAs) across GPCs and SMs.                  │ │
│  │  Manages scheduling, load balancing, context switching between kernels. │ │
│  └────────────────────────────────┬────────────────────────────────────────┘ │
│                                   │                                          │
│  ┌────────────────────────────────▼────────────────────────────────────────┐ │
│  │                         8 GPCs (Graphics Processing Clusters)           │ │
│  │                                                                          │ │
│  │  ┌──────────────────────────────────────────────────────────────────┐   │ │
│  │  │  GPC 0                          GPC 1                            │   │ │
│  │  │  ┌──────────────────────────┐  ┌──────────────────────────┐     │   │ │
│  │  │  │  Raster Engine           │  │  Raster Engine           │     │   │ │
│  │  │  │  (graphics only, idle    │  │  (graphics only, idle    │     │   │ │
│  │  │  │   during compute)        │  │   during compute)        │     │   │ │
│  │  │  │                          │  │                          │     │   │ │
│  │  │  │  ┌────┐┌────┐           │  │  ┌────┐┌────┐           │     │   │ │
│  │  │  │  │SM 0││SM 1│           │  │  │SM14││SM15│           │     │   │ │
│  │  │  │  └────┘└────┘           │  │  └────┘└────┘           │     │   │ │
│  │  │  │  ┌────┐┌────┐           │  │  ┌────┐┌────┐           │     │   │ │
│  │  │  │  │SM 2││SM 3│  ...      │  │  │SM16││SM17│  ...      │     │   │ │
│  │  │  │  └────┘└────┘           │  │  └────┘└────┘           │     │   │ │
│  │  │  │  ...                     │  │  ...                     │     │   │ │
│  │  │  │  ┌────┐┌────┐           │  │  ┌────┐┌────┐           │     │   │ │
│  │  │  │  │SM12││SM13│           │  │  │SM26││SM27│           │     │   │ │
│  │  │  │  └────┘└────┘           │  │  └────┘└────┘           │     │   │ │
│  │  │  │  (14 SMs per GPC)       │  │  (14 SMs per GPC)       │     │   │ │
│  │  │  └──────────────────────────┘  └──────────────────────────┘     │   │ │
│  │  │                                                                  │   │ │
│  │  │  GPC 2 ... GPC 7  (6 more GPCs, same structure)                 │   │ │
│  │  │                                                                  │   │ │
│  │  │  Total: 8 GPCs × ~14 SMs = 108 SMs (A100 has 128, 20 disabled) │   │ │
│  │  └──────────────────────────────────────────────────────────────────┘   │ │
│  └────────────────────────────────┬────────────────────────────────────────┘ │
│                                   │                                          │
│  ┌────────────────────────────────▼────────────────────────────────────────┐ │
│  │                         L2 Cache — 40 MB                                │ │
│  │  Shared by ALL 108 SMs. Hardware-managed (you don't control it).       │ │
│  │  Partitioned into slices, each slice associated with an HBM channel.   │ │
│  │  Caches global memory reads. Reduces HBM traffic.                      │ │
│  │  Bandwidth: ~5 TB/s (much faster than HBM)                             │ │
│  └────────────────────────────────┬────────────────────────────────────────┘ │
│                                   │                                          │
│  ┌────────────────────────────────▼────────────────────────────────────────┐ │
│  │                    Memory Controllers (8 channels)                      │ │
│  │  Each controller connects to a stack of HBM2e chips.                   │ │
│  │  8 channels × 256-bit bus = 5120-bit total memory bus.                 │ │
│  └────────────────────────────────┬────────────────────────────────────────┘ │
│                                   │                                          │
│  ┌────────────────────────────────▼────────────────────────────────────────┐ │
│  │                     HBM2e (High Bandwidth Memory)                       │ │
│  │                                                                          │ │
│  │   ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐       │ │
│  │   │Stack 0│ │Stack 1│ │Stack 2│ │Stack 3│ │Stack 4│ │Stack 5│       │ │
│  │   │10 GB  │ │10 GB  │ │10 GB  │ │10 GB  │ │10 GB  │ │10 GB  │ ...  │ │
│  │   └───────┘ └───────┘ └───────┘ └───────┘ └───────┘ └───────┘       │ │
│  │                                                                          │ │
│  │   8 stacks × 10 GB = 80 GB total                                       │ │
│  │   Aggregate bandwidth: 2,039 GB/s (~2 TB/s)                            │ │
│  │                                                                          │ │
│  │   HBM sits NEXT TO the GPU die (on the same package substrate)         │ │
│  │   Connected by thousands of tiny wires (silicon interposer)            │ │
│  │   This physical proximity is why bandwidth is so high vs DDR5          │ │
│  │                                                                          │ │
│  │   ┌──────────────────────────────────────────────────┐                  │ │
│  │   │  Physical layout (top-down view of the package): │                  │ │
│  │   │                                                   │                  │ │
│  │   │  ┌─────┐ ┌─────┐                   ┌─────┐ ┌─────┐│                 │ │
│  │   │  │HBM 0│ │HBM 1│   ┌───────────┐   │HBM 4│ │HBM 5││                 │ │
│  │   │  └─────┘ └─────┘   │           │   └─────┘ └─────┘│                 │ │
│  │   │                     │  GPU Die  │                   │                 │ │
│  │   │  ┌─────┐ ┌─────┐   │ (826 mm²) │   ┌─────┐ ┌─────┐│                 │ │
│  │   │  │HBM 2│ │HBM 3│   │           │   │HBM 6│ │HBM 7││                 │ │
│  │   │  └─────┘ └─────┘   └───────────┘   └─────┘ └─────┘│                 │ │
│  │   │                                                      │                │ │
│  │   │  Everything on one silicon interposer substrate      │                │ │
│  │   └──────────────────────────────────────────────────────┘                │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
│  Also on the chip:                                                           │
│   • Copy Engines (DMA): handle CPU↔GPU and GPU↔GPU data transfers           │
│     (run in parallel with compute — this enables CUDA stream overlap)       │
│   • Video codec: NVDEC (decode) + NVENC (encode) for video processing       │
│   • Power management: clock gating, voltage/frequency scaling               │
│   • Debug/profiling: performance counters, warp stall reasons               │
│                                                                               │
│  Die size: 826 mm² (A100), 814 mm² (H100) — close to reticle limit!        │
│  Transistors: 54 billion (A100), 80 billion (H100)                          │
│  Process: TSMC 7nm (A100), TSMC 4N (H100)                                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

### How a Kernel Launch Flows Through the Chip

```
CPU calls: kernel<<<num_blocks, threads_per_block>>>(args)
     │
     ▼
PCIe/NVLink → GPU Command Queue
     │
     ▼
GigaThread Engine
  "This kernel has 4000 blocks, each 256 threads"
  "I have 108 SMs available"
  → Assign blocks to SMs (each SM can run multiple blocks concurrently)
  → SM 0 gets blocks [0, 108, 216, ...]
  → SM 1 gets blocks [1, 109, 217, ...]
  → ...
     │
     ▼
Each SM receives its assigned block:
  256 threads → 8 warps
  Warp schedulers pick warps to execute each cycle
  Instructions fetch data: registers → shared mem → L2 → HBM
  When a warp stalls on memory → instantly switch to another warp
     │
     ▼
Results written to global memory (HBM)
  → Copy Engine (DMA) transfers results back to CPU over PCIe
  → Or stays on GPU for the next kernel
```

### Blocks vs SMs — Software Model vs Hardware Model

```
The CUDA programming model has a SOFTWARE hierarchy (what you write)
and the GPU has a HARDWARE hierarchy (what actually runs it).
The runtime MAPS software onto hardware:

  SOFTWARE (you control)              HARDWARE (fixed)
  ──────────────────────              ──────────────────
  Grid (all blocks)          ───►    GPU (entire chip)
  Block (256 threads)        ───►    SM (Streaming Multiprocessor)
  Warp (32 threads)          ───►    32 CUDA cores in lockstep
  Thread (1 thread)          ───►    1 CUDA core (1 lane)

  ┌──────────────────────────────────────────────────────────────────┐
  │  You write:                   Hardware does:                      │
  │                                                                   │
  │  kernel<<<4000, 256>>>()      GigaThread Engine takes 4000 blocks│
  │       ^^^^  ^^^                and distributes them across 108 SMs│
  │       │     └ threads/block                                       │
  │       └ blocks                                                    │
  │                                                                   │
  │  Block 0   ──► assigned to SM 0                                   │
  │  Block 1   ──► assigned to SM 1                                   │
  │  Block 2   ──► assigned to SM 2                                   │
  │  ...                                                              │
  │  Block 107 ──► assigned to SM 107                                 │
  │  Block 108 ──► assigned to SM 0  (SM 0 now runs 2 blocks!)       │
  │  Block 109 ──► assigned to SM 1                                   │
  │  ...                                                              │
  │  Block 3999 ──► assigned to SM (3999 % 108)                      │
  │                                                                   │
  │  You DON'T choose which SM runs which block. Hardware decides.   │
  │  You DON'T even know how many SMs exist when writing the kernel. │
  │  That's the whole point: your code scales to ANY GPU.            │
  └──────────────────────────────────────────────────────────────────┘

  Why this separation matters:

  1. PORTABILITY — same kernel runs on any GPU
     A100: 108 SMs → 108 blocks run in parallel, 4000 blocks finish in ~37 waves
     RTX 3060: 28 SMs → 28 blocks in parallel, 4000 blocks finish in ~143 waves
     Same code, same result. Just different speed.

  2. SCALING — launch more blocks than SMs
     You don't need to know the hardware. Launch enough blocks to
     cover your data. The runtime fills SMs as blocks complete.

     ┌───────────────────────────────────────────────────────────┐
     │ Time ──────────────────────────────────────────────────►  │
     │                                                           │
     │ SM 0: [Block 0][Block 108][Block 216]...[Block 3888]     │
     │ SM 1: [Block 1][Block 109][Block 217]...[Block 3889]     │
     │ SM 2: [Block 2][Block 110][Block 218]...[Block 3890]     │
     │ ...                                                       │
     │ SM 107:[Block 107][Block 215][Block 323]...[Block 3999]  │
     │                                                           │
     │ Blocks are queued. As soon as an SM finishes a block,    │
     │ it immediately starts the next one. No idle SMs.          │
     └───────────────────────────────────────────────────────────┘

  3. RESOURCE ISOLATION — blocks on the same SM share SM resources
     Each SM has: 256 KB registers, 192 KB shared memory, 64 warps max.
     Each block RESERVES some of these when it's assigned to the SM.

     If your block uses:
       64 KB shared memory → SM can fit 3 blocks (3 × 64 = 192 KB)
       128 registers/thread × 256 threads = 32 KB regs → limited by regs or shared mem

     Fewer resources per block → more blocks per SM → more latency hiding.
     This is why kernel optimization often means REDUCING resource usage.
```

### Zooming In — Inside One SM

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

### SIMT — Single Instruction Multiple Threads (the GPU execution model)

```
You've heard of SIMD (Single Instruction Multiple Data) from CPUs:
  AVX2: one instruction operates on 8 floats in a 256-bit register
  All 8 lanes do the EXACT same thing. No exceptions.

SIMT is the GPU's version of this, but more flexible:
  One instruction operates on 32 threads (a warp)
  BUT: individual threads can be ACTIVATED or DEACTIVATED
  This means SIMT can handle branching. SIMD cannot (easily).

  ┌───────────────────────────────────────────────────────────────┐
  │                    SIMD vs SIMT                                │
  ├───────────────────────────────────────────────────────────────┤
  │                                                                │
  │  CPU SIMD (AVX2):                                              │
  │    vaddps ymm0, ymm1, ymm2    ← ALL 8 lanes add. Always.     │
  │    No branching possible within the 8 lanes.                   │
  │    (AVX-512 adds mask registers — limited per-lane control)   │
  │                                                                │
  │  GPU SIMT (warp of 32):                                        │
  │    add  r1, r2, r3            ← 32 threads execute this       │
  │    BUT: thread 5, 12, 27 can be MASKED OFF (inactive)         │
  │    They still "ride along" but produce no result.              │
  │    Next instruction: a different set can be active.            │
  │                                                                │
  │  Key difference:                                               │
  │    SIMD: you write VECTOR code (load 8 floats, add 8 floats)  │
  │    SIMT: you write SCALAR code, GPU runs it on 32 threads     │
  │          Each thread executes the kernel as if it's alone.     │
  │          The hardware parallelizes it into warps.              │
  └───────────────────────────────────────────────────────────────┘
```

**How SIMT handles branching (the key insight):**

```
Consider this kernel code running on a warp of 32 threads:

  if (threadIdx.x % 2 == 0) {
      A();    // even threads
  } else {
      B();    // odd threads
  }
  Z();        // all threads

On a CPU, different threads would just branch independently.
On a GPU, all 32 threads in a warp share ONE instruction pointer.

What actually happens:

  Time ──────────────────────────────────────────────────►

  Step 1: Evaluate condition
    Thread 0:  true     Thread 1:  false    Thread 2:  true  ...
    Thread 16: true     Thread 17: false    Thread 18: true  ...

  Step 2: Execute if-branch (A), mask off odd threads
    ┌─────────────────────────────────────────────────┐
    │ T0: A()  T1: ░░░  T2: A()  T3: ░░░  T4: A() │
    │ T5: ░░░  T6: A()  T7: ░░░  T8: A()  ...     │
    │                                                 │
    │ ░░░ = thread INACTIVE (masked off, doing nothing)│
    │ 16 out of 32 threads are idle = 50% utilization │
    └─────────────────────────────────────────────────┘

  Step 3: Execute else-branch (B), mask off even threads
    ┌─────────────────────────────────────────────────┐
    │ T0: ░░░  T1: B()  T2: ░░░  T3: B()  T4: ░░░ │
    │ T5: B()  T6: ░░░  T7: B()  T8: ░░░  ...     │
    │                                                 │
    │ The OTHER 16 threads now idle.                   │
    └─────────────────────────────────────────────────┘

  Step 4: Reconverge — all threads execute Z()
    ┌─────────────────────────────────────────────────┐
    │ T0: Z()  T1: Z()  T2: Z()  T3: Z()  T4: Z() │
    │ All 32 threads active again.                     │
    └─────────────────────────────────────────────────┘

  Result: CORRECT but SLOW.
    if-branch and else-branch execute SEQUENTIALLY, not in parallel.
    Total time = time(A) + time(B) + time(Z)    not max(A,B) + Z
    This is called WARP DIVERGENCE.
```

**Warp divergence at the instruction level — what the hardware actually does:**

```
Source code:
  if (threadIdx.x % 2 == 0) {
      y = a + b;      // even threads
  } else {
      y = a * b;      // odd threads
  }
  z = y + 1;          // all threads

This compiles to something like:

  Addr  Instruction         What it does
  ───── ──────────────────  ─────────────────────────────────
  0x00  AND  r4, tid, 1    r4 = threadIdx.x & 1  (0 or 1)
  0x04  SETP p0, r4, 0     p0 = (r4 == 0)  → predicate register
  0x08  @!p0 BRA 0x14      if p0 is FALSE → jump to else-branch
  ─── if-branch (even threads) ───
  0x0C  ADD  r5, r1, r2    y = a + b
  0x10  BRA  0x18           jump past else-branch → reconverge
  ─── else-branch (odd threads) ───
  0x14  MUL  r5, r1, r2    y = a * b
  ─── reconverge point ───
  0x18  ADD  r6, r5, 1     z = y + 1

Now, on a CPU with 2 threads, this is fine:
  Thread 0 (even): 0x00 → 0x04 → 0x08(not taken) → 0x0C → 0x10 → 0x18
  Thread 1 (odd):  0x00 → 0x04 → 0x08(taken)     → 0x14 → 0x18
  They run independently on separate cores. No problem.

On a GPU, all 32 threads in a warp share ONE program counter.
Here's what really happens cycle by cycle for 4 threads (T0-T3):

  Cycle  PC     Instruction       T0(even)  T1(odd)  T2(even)  T3(odd)
  ─────  ─────  ────────────────  ────────  ───────  ────────  ───────
    1    0x00   AND r4, tid, 1    ✓ r4=0   ✓ r4=1  ✓ r4=0   ✓ r4=1
    2    0x04   SETP p0, r4, 0   ✓ p0=T   ✓ p0=F  ✓ p0=T   ✓ p0=F
    3    0x08   @!p0 BRA 0x14    (not tkn) (taken)  (not tkn) (taken)
                                  ─── DIVERGENCE DETECTED! ───
                                  Hardware splits warp into 2 groups.
                                  Sets active mask = 0b...1010 (even)

  ── execute if-branch with even threads active ──
    4    0x0C   ADD r5, r1, r2   ✓ ADD    ░░░ idle ✓ ADD    ░░░ idle
    5    0x10   BRA 0x18         ✓ jump   ░░░      ✓ jump   ░░░

                                  Sets active mask = 0b...0101 (odd)

  ── execute else-branch with odd threads active ──
    6    0x14   MUL r5, r1, r2   ░░░ idle ✓ MUL   ░░░ idle ✓ MUL

                                  Restore active mask = 0b...1111 (all)

  ── reconverge: all threads active again ──
    7    0x18   ADD r6, r5, 1    ✓ ADD    ✓ ADD    ✓ ADD    ✓ ADD

  Total: 7 cycles.
  Without divergence (all threads take same branch): 5 cycles.
  The extra 2 cycles are PURE WASTE — idle threads burning silicon.

  KEY INSIGHT: The BRA (branch/jump) instruction is where it all goes wrong.
  On a CPU, a branch sends each thread to a different address. Fine.
  On a GPU, a branch can't send 32 threads to different addresses,
  because there's only ONE program counter per warp.

  So the hardware says: "OK, I'll go to BOTH addresses, one at a time,
  and mask off the threads that shouldn't be running."

  This is why warp switching (warp 0 → warp 3) is FREE (zero cost),
  but thread divergence WITHIN a warp is EXPENSIVE:
    - Warp switch: just pick a different warp, it has its own PC & regs.
    - Divergence:  the 32 threads inside are STUCK TOGETHER. You can't
                   split them onto different PCs. You serialize instead.
```

**Volta+ improvement (independent thread scheduling):**

```
Pre-Volta GPUs (Pascal and earlier):
  Entire if-block must finish → then entire else-block → then sync all.

  Time:  [---- A() for all even threads ----][---- B() for all odd ----][sync][Z()]

Volta and later (V100, A100, H100):
  Each thread has its own program counter + call stack.
  Threads can interleave at finer granularity:

  Time:  [A-stmt1, B-stmt1] [A-stmt2, B-stmt2] [Z()]
         (even threads do A  (even do A-stmt2,   (all)
          odd do B-stmt1)     odd do B-stmt2)

  Still not TRUE parallel (still one instruction at a time per warp),
  but can synchronize at intermediate points — better for complex code
  with shared data between branches.
```

### Warps — The 32-Thread Execution Bundle

```
A warp = 32 threads that execute in lockstep on 32 CUDA cores.

Why 32? That's NVIDIA's hardware width. Just like AVX2 is 8 floats
wide because Intel's hardware is 256 bits. It's a fixed hardware choice.

How threads are grouped into warps:

  Block of 256 threads:
  ┌──────────────────────────────────────────────────────┐
  │ Warp 0:  threads  0-31                                │
  │ Warp 1:  threads 32-63                                │
  │ Warp 2:  threads 64-95                                │
  │ ...                                                    │
  │ Warp 7:  threads 224-255                              │
  └──────────────────────────────────────────────────────┘
  = 8 warps per block (256 / 32 = 8)

  An SM can hold MANY warps simultaneously (up to 64 on A100).
  This is how GPUs hide latency:

  ┌────────────────────────────────────────────────────────────────┐
  │ SM Warp Scheduler                                              │
  │                                                                │
  │ Cycle 1: Warp 0 issues instruction (add)                       │
  │ Cycle 2: Warp 0 waiting for memory → switch to Warp 3          │
  │ Cycle 3: Warp 3 issues instruction (mul)                       │
  │ Cycle 4: Warp 3 waiting → switch to Warp 7                     │
  │ Cycle 5: Warp 7 issues instruction → Warp 0's data is ready!  │
  │ Cycle 6: Warp 0 continues                                      │
  │                                                                │
  │ Unlike CPU (saves/restores registers on context switch):       │
  │ GPU keeps ALL warp states resident. Zero-cost warp switching!  │
  │ Each warp has its own registers — no save/restore needed.      │
  └────────────────────────────────────────────────────────────────┘

  This is called LATENCY HIDING.
  CPU hides latency with: out-of-order execution, speculation, caches
  GPU hides latency with: switching to another warp instantly
  More warps in flight = more latency hiding = higher utilization

  Occupancy = active warps / maximum warps per SM
    Low occupancy (< 25%):  not enough warps to hide memory latency
    Good occupancy (> 50%): usually enough to keep SM busy
    100% occupancy:         not always needed — depends on memory patterns
```

**Why warps matter for your code:**

```
1. WARP DIVERGENCE (branching)
   Threads in the same warp that take different branches
   → serialized execution → wasted cycles.

   Bad (50% divergence):
     if (threadIdx.x % 2 == 0) { A(); } else { B(); }
     // Threads 0,2,4,... vs 1,3,5,... in SAME warp → diverge

   Good (0% divergence):
     if (threadIdx.x < 16) { A(); } else { B(); }
     // Wait, this is ALSO divergent within a warp!

   Actually good (0% divergence):
     if (warpId % 2 == 0) { A(); } else { B(); }
     // Entire warp-0 does A, entire warp-1 does B. No divergence.
     // Each warp is uniform → both branches execute at full speed.

   Rule: branch at WARP granularity (multiples of 32), not per-thread.

2. MEMORY COALESCING
   When 32 threads in a warp access memory, the hardware tries to
   combine their requests into as few memory transactions as possible.

   Coalesced (1 transaction for 32 threads):
     data[threadIdx.x]       // threads 0-31 access consecutive addresses
     → ONE 128-byte memory transaction. Fast!

   Uncoalesced (32 separate transactions!):
     data[threadIdx.x * stride]  // strided access, gaps between addresses
     → Up to 32 separate transactions. 32x slower!

     data[random_index[threadIdx.x]]  // random access
     → 32 separate transactions. Terrible.

   ┌────────────────────────────────────────────────────────────┐
   │ Warp memory access patterns:                               │
   │                                                            │
   │  Coalesced (fast):    ████████████████████████████████     │
   │  Thread 0→addr 0, 1→addr 1, 2→addr 2, ... contiguous     │
   │  → 1 memory transaction                                    │
   │                                                            │
   │  Strided (slow):     █   █   █   █   █   █   █   █       │
   │  Thread 0→addr 0, 1→addr 4, 2→addr 8, ... gaps           │
   │  → multiple transactions, wasted bandwidth                 │
   │                                                            │
   │  Random (terrible):  █ █    █  █  █    █   ██   █         │
   │  → up to 32 separate transactions                          │
   └────────────────────────────────────────────────────────────┘

3. WARP-LEVEL PRIMITIVES (modern CUDA)
   Threads within a warp can communicate WITHOUT shared memory:

   __shfl_sync(mask, val, srcLane)   // read another thread's register
   __ballot_sync(mask, predicate)    // which threads have predicate=true?
   __reduce_sync(mask, val, op)      // warp-wide reduction (sum, min, max)

   These are REGISTER-to-REGISTER, no memory involved.
   Used heavily in: reduction kernels, scan (prefix sum), attention.

   Example — warp-level sum (no shared memory needed):
     // Each thread has a value. Sum all 32 values.
     for (int offset = 16; offset > 0; offset /= 2)
         val += __shfl_down_sync(0xffffffff, val, offset);
     // Thread 0 now has the sum. 5 shuffle instructions, ~5 cycles.
     // vs shared memory approach: ~30 cycles (write, sync, read, sync...)
```

## 3. Programming Model (CUDA Hierarchy)

```
┌──────────────────────────────────────────────────────────────────────────┐
│                    CUDA Programming Model + Memory                        │
│                                                                           │
│  ┌──────────────────────────────────────────────────────────────────┐    │
│  │  GLOBAL MEMORY (HBM) — visible to ALL threads in ALL blocks      │    │
│  │  80 GB @ 2-3 TB/s.  Slowest GPU memory. Persists across kernels. │    │
│  │                                                                   │    │
│  │  Grid (entire kernel launch)                                      │    │
│  │  ┌────────────────────────────────────────────────────────────┐  │    │
│  │  │                                                            │  │    │
│  │  │  Block (0,0)              Block (1,0)                      │  │    │
│  │  │  ┌──────────────────────┐ ┌──────────────────────┐        │  │    │
│  │  │  │ SHARED MEMORY 192KB │ │ SHARED MEMORY 192KB │        │  │    │
│  │  │  │ (visible to all      │ │ (visible to all      │        │  │    │
│  │  │  │  threads in THIS     │ │  threads in THIS     │        │  │    │
│  │  │  │  block only)         │ │  block only)         │        │  │    │
│  │  │  │ ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ │ │ ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ │        │  │    │
│  │  │  │ Warp 0 (T0-T31)     │ │ Warp 0 (T0-T31)     │        │  │    │
│  │  │  │ ┌────┐┌────┐┌────┐  │ │ ┌────┐┌────┐┌────┐  │        │  │    │
│  │  │  │ │ T0 ││ T1 ││... │  │ │ │ T0 ││ T1 ││... │  │        │  │    │
│  │  │  │ │regs││regs││    │  │ │ │regs││regs││    │  │        │  │    │
│  │  │  │ └────┘└────┘└────┘  │ │ └────┘└────┘└────┘  │        │  │    │
│  │  │  │ Warp 1 (T32-T63)    │ │ Warp 1 (T32-T63)    │        │  │    │
│  │  │  │ ┌────┐┌────┐┌────┐  │ │ ┌────┐┌────┐┌────┐  │        │  │    │
│  │  │  │ │T32 ││T33 ││... │  │ │ │T32 ││T33 ││... │  │        │  │    │
│  │  │  │ │regs││regs││    │  │ │ │regs││regs││    │  │        │  │    │
│  │  │  │ └────┘└────┘└────┘  │ │ └────┘└────┘└────┘  │        │  │    │
│  │  │  │ ...                  │ │ ...                  │        │  │    │
│  │  │  │ Warp 7 (T224-T255)  │ │ Warp 7 (T224-T255)  │        │  │    │
│  │  │  └──────────────────────┘ └──────────────────────┘        │  │    │
│  │  │                                                            │  │    │
│  │  │  Block (0,1)              Block (1,1)                      │  │    │
│  │  │  ┌──────────────────────┐ ┌──────────────────────┐        │  │    │
│  │  │  │ SHARED MEMORY 192KB │ │ SHARED MEMORY 192KB │        │  │    │
│  │  │  │ (its OWN copy —      │ │ (its OWN copy —      │        │  │    │
│  │  │  │  cannot see Block     │ │  cannot see Block     │        │  │    │
│  │  │  │  (0,0)'s shared mem) │ │  (1,0)'s shared mem) │        │  │    │
│  │  │  │ ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ │ │ ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈ │        │  │    │
│  │  │  │ 256 threads (8 warps)│ │ 256 threads (8 warps)│        │  │    │
│  │  │  │ each with own regs   │ │ each with own regs   │        │  │    │
│  │  │  └──────────────────────┘ └──────────────────────┘        │  │    │
│  │  └────────────────────────────────────────────────────────────┘  │    │
│  └──────────────────────────────────────────────────────────────────┘    │
│                                                                           │
│  Memory scope summary:                                                    │
│                                                                           │
│    Registers  → per THREAD  (fastest, ~20 TB/s, 255 regs per thread)     │
│                 T0 cannot read T1's registers (unless warp shuffle)       │
│                                                                           │
│    Shared Mem → per BLOCK   (fast, ~15 TB/s, 192 KB per SM)             │
│                 All threads in Block(0,0) share one pool.                 │
│                 Block(0,0) CANNOT see Block(1,0)'s shared memory.        │
│                 This is why __syncthreads() only syncs within a block.   │
│                                                                           │
│    Global Mem → per GRID    (slow, 2-3 TB/s, 80 GB HBM)                 │
│                 ALL blocks can read/write. This is how blocks communicate.│
│                 But no ordering guarantees — need atomics or barriers.    │
│                                                                           │
│    L2 Cache   → hardware-managed, sits between SMs and Global Memory     │
│                 You don't control it. GPU auto-caches global mem reads.   │
│                                                                           │
│  Why this matters:                                                        │
│    Shared memory is 10-15x faster than global memory.                    │
│    The tiled matmul kernel loads tiles into shared memory so that        │
│    all 256 threads in the block can reuse the same data from the tile,  │
│    instead of each thread loading from slow global memory individually.  │
│    But threads in different blocks MUST go through global memory.        │
└──────────────────────────────────────────────────────────────────────────┘
```

### Writing a Real CUDA Kernel — From Code to Execution

```c
// vector_add.cu — the "hello world" of GPU programming

// DEVICE code (runs on GPU). __global__ = callable from CPU.
__global__ void vector_add(float* a, float* b, float* c, int n) {
    // Each thread computes ONE element.
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    //       ^^^^^^^^    ^^^^^^^^      ^^^^^^^^^^
    //       which block  threads/block  which thread in block
    //       (0, 1, 2..)  (256)         (0..255)

    if (i < n) {          // bounds check (grid may be larger than data)
        c[i] = a[i] + b[i];
    }
}

// HOST code (runs on CPU). Orchestrates everything.
int main() {
    int n = 1000000;
    size_t bytes = n * sizeof(float);

    // 1. Allocate CPU memory
    float *h_a = (float*)malloc(bytes);
    float *h_b = (float*)malloc(bytes);
    float *h_c = (float*)malloc(bytes);
    // ... fill h_a, h_b with data ...

    // 2. Allocate GPU memory
    float *d_a, *d_b, *d_c;
    cudaMalloc(&d_a, bytes);
    cudaMalloc(&d_b, bytes);
    cudaMalloc(&d_c, bytes);

    // 3. Copy data CPU → GPU
    cudaMemcpy(d_a, h_a, bytes, cudaMemcpyHostToDevice);
    cudaMemcpy(d_b, h_b, bytes, cudaMemcpyHostToDevice);

    // 4. Launch kernel  <<<grid_size, block_size>>>
    int threads_per_block = 256;
    int blocks = (n + threads_per_block - 1) / threads_per_block;  // ceil division
    vector_add<<<blocks, threads_per_block>>>(d_a, d_b, d_c, n);

    // 5. Copy result GPU → CPU
    cudaMemcpy(h_c, d_c, bytes, cudaMemcpyDeviceToHost);

    // 6. Free GPU memory
    cudaFree(d_a); cudaFree(d_b); cudaFree(d_c);
    free(h_a); free(h_b); free(h_c);
}

// Compile and run:
//   nvcc vector_add.cu -o vector_add && ./vector_add
```

```
What happens at launch:

  vector_add<<<3907, 256>>>(d_a, d_b, d_c, 1000000);

  Grid: 3907 blocks
  Each block: 256 threads = 8 warps
  Total: 3907 × 256 = 999,936 threads (covers 1M elements)

  Scheduler distributes blocks across 108 SMs (A100):
    Each SM gets ~36 blocks over time
    Each SM runs multiple blocks concurrently (limited by registers/shared mem)

  Each thread:
    Computes its global index: i = blockIdx.x * 256 + threadIdx.x
    Loads a[i] and b[i] from HBM (coalesced — consecutive threads, consecutive addresses)
    Adds them
    Stores c[i] to HBM

  Total time for 1M floats on A100: ~0.02 ms (dominated by launch overhead)
```

### A More Realistic Kernel — Matrix Multiply with Shared Memory

```c
// Tiled matrix multiply: C = A × B
// Uses shared memory to reduce HBM accesses
// This is a simplified version — production uses Tensor Cores via cuBLAS

#define TILE 16

__global__ void matmul(float* A, float* B, float* C, int N) {
    // Shared memory tiles — visible to all threads in this block
    __shared__ float sA[TILE][TILE];
    __shared__ float sB[TILE][TILE];

    int row = blockIdx.y * TILE + threadIdx.y;
    int col = blockIdx.x * TILE + threadIdx.x;
    float sum = 0.0f;

    // Slide tile across the K dimension
    for (int t = 0; t < N / TILE; t++) {
        // Each thread loads ONE element into shared memory
        sA[threadIdx.y][threadIdx.x] = A[row * N + t * TILE + threadIdx.x];
        sB[threadIdx.y][threadIdx.x] = B[(t * TILE + threadIdx.y) * N + col];
        __syncthreads();  // wait for all threads to finish loading

        // Compute partial dot product using data in shared memory (fast!)
        for (int k = 0; k < TILE; k++)
            sum += sA[threadIdx.y][k] * sB[k][threadIdx.x];
        __syncthreads();  // wait before loading next tile
    }

    C[row * N + col] = sum;
}

// Launch: 2D grid of 2D blocks
//   dim3 block(TILE, TILE);           // 16×16 = 256 threads per block
//   dim3 grid(N / TILE, N / TILE);    // covers entire output matrix
//   matmul<<<grid, block>>>(d_A, d_B, d_C, N);

// Why shared memory matters here:
//   Without tiling: each output element loads 2N floats from HBM
//     N=4096 → 32 KB per element × 16M elements = insane bandwidth
//   With tiling: each element reuses data loaded by its block-mates
//     Reduces HBM access by TILE × (16x in this case)
//   In practice: use cuBLAS (calls Tensor Cores, 10-50x faster than this)
```

### PyTorch — How Most People Actually Use GPUs

```python
# In practice you rarely write CUDA kernels directly.
# PyTorch handles GPU memory, transfers, and kernel dispatch.

import torch

# Move data to GPU (like cudaMemcpy but automatic)
x = torch.randn(1000, 1000, device='cuda')   # allocated directly on GPU
y = torch.randn(1000, 1000, device='cuda')

# Operations dispatch to cuBLAS/cuDNN kernels automatically
z = x @ y                    # matmul → cuBLAS → Tensor Cores
z = torch.relu(z)            # element-wise → CUDA kernel
z = torch.softmax(z, dim=1)  # reduction → CUDA kernel

# torch.compile (PyTorch 2.0+): fuses multiple ops into one kernel
@torch.compile
def fused_op(x, y):
    z = x @ y
    z = torch.relu(z)
    return torch.softmax(z, dim=1)
    # Without compile: 3 separate kernel launches (launch overhead)
    # With compile:    1 fused kernel (Triton/inductor generates CUDA)

# Move result back to CPU
result = z.cpu().numpy()

# The stack:
#   Your Python code
#     → PyTorch (Python + C++ dispatcher)
#       → cuBLAS (matmul), cuDNN (conv), custom CUDA kernels
#         → CUDA runtime → GPU driver → hardware
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

## 9. Heterogeneous Applications — CPU + GPU Working Together

A real application is never "all GPU." It's a heterogeneous system where
CPU and GPU each handle what they're good at, and the challenge is
orchestrating the data flow between them.

### The Execution Model

```
A heterogeneous application has two kinds of code:

  HOST code:   runs on CPU (sequential, branchy, orchestration)
  DEVICE code: runs on GPU (parallel kernels)

The CPU is always the ORCHESTRATOR. Even for "GPU workloads,"
the CPU is doing:
  - Reading input data from disk/network
  - Preprocessing (parsing, tokenization, batching)
  - Deciding WHICH kernels to launch and in WHAT order
  - Collecting results from GPU
  - Postprocessing (decoding, formatting response)

  ┌──────────────────────────────────────────────────────────────┐
  │ Timeline of a typical ML inference request:                   │
  │                                                               │
  │ CPU │████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████████│  │
  │     │receive │                                    │ send   │  │
  │     │request,│     wait for GPU                   │response│  │
  │     │preproc │                                    │postproc│  │
  │     │tokenize│                                    │decode  │  │
  │                                                               │
  │ GPU │░░░░░░░░████████████████████████████████████░░░░░░░░░│  │
  │     │  idle  │ kernel 1 │ kernel 2 │ kernel 3   │  idle   │  │
  │     │(waiting│(attention)│ (FFN)    │(softmax)   │(done)   │  │
  │     │for data│          │          │            │         │  │
  │                                                               │
  │ PCIe│        ─►copy in              copy out◄─              │  │
  │     │        input                  output                  │  │
  └──────────────────────────────────────────────────────────────┘
```

### The PCIe Bottleneck

```
CPU and GPU have SEPARATE memory. Data must be copied between them.

  CPU (Host)                           GPU (Device)
  ┌──────────────┐                    ┌──────────────┐
  │ System RAM   │ ◄──── PCIe ─────► │ HBM (VRAM)   │
  │ (DDR5)       │   bus             │              │
  │ 64-512 GB    │   32 GB/s (Gen4)  │ 80 GB        │
  │ ~100 GB/s BW │   64 GB/s (Gen5)  │ ~3 TB/s BW   │
  └──────────────┘                    └──────────────┘

  PCIe bandwidth is 50-100x slower than GPU's internal memory BW.
  This is THE fundamental bottleneck of heterogeneous computing.

  Copying 1 GB over PCIe Gen4: ~31 ms
  Processing that 1 GB on GPU:  ~0.3 ms (at 3 TB/s bandwidth)
  → Transfer takes 100x longer than compute!

  The ratio: compute_time / transfer_time is critical.
  If transfer dominates → GPU sits idle most of the time → wasteful.
  If compute dominates → PCIe transfer is negligible → good.

  Rule of thumb:
    Small data + heavy compute (matrix multiply) → GPU wins big
    Large data + light compute (element-wise add) → PCIe kills you
```

### Hiding Transfer Latency — Streams and Overlap

```
Naive approach (sequential):
  Copy batch 1 to GPU → Compute batch 1 → Copy result back
  Copy batch 2 to GPU → Compute batch 2 → Copy result back

  Time: [====copy====][====compute====][====copy back====]
        [====copy====][====compute====][====copy back====]
  GPU idle during every copy. Bad.

Smart approach (overlapping with CUDA streams):
  Stream 1: Copy batch 1 → Compute batch 1 → Copy back
  Stream 2:     Copy batch 2 → Compute batch 2 → Copy back
  Stream 3:         Copy batch 3 → Compute batch 3 → Copy back

  Time: [==copy 1==][==copy 2==][==copy 3==]
                    [==comp 1==][==comp 2==][==comp 3==]
                                [==back 1==][==back 2==][==back 3==]

  GPU copy engine and compute engine run IN PARALLEL.
  GPU is never idle after warmup. Transfer is fully hidden.

  In CUDA:
    cudaStream_t s1, s2;
    cudaStreamCreate(&s1);
    cudaStreamCreate(&s2);

    // Stream 1: copy + compute
    cudaMemcpyAsync(d_a, h_a, size, cudaMemcpyHostToDevice, s1);
    kernel<<<blocks, threads, 0, s1>>>(d_a, d_out1);
    cudaMemcpyAsync(h_out1, d_out1, size, cudaMemcpyDeviceToHost, s1);

    // Stream 2: overlapping copy + compute
    cudaMemcpyAsync(d_b, h_b, size, cudaMemcpyHostToDevice, s2);
    kernel<<<blocks, threads, 0, s2>>>(d_b, d_out2);
    cudaMemcpyAsync(h_out2, d_out2, size, cudaMemcpyDeviceToHost, s2);

  Requirement: host memory must be PAGE-LOCKED (pinned):
    cudaMallocHost(&h_a, size);  // pinned memory
    // Regular malloc'd memory can't be used with async copy
    // because the OS might swap it out mid-transfer.
```

### Unified Memory — Let the Hardware Manage Transfers

```
Explicit memory management (traditional):
  cudaMalloc(&d_ptr, size);
  cudaMemcpy(d_ptr, h_ptr, size, cudaMemcpyHostToDevice);
  kernel<<<...>>>(d_ptr);
  cudaMemcpy(h_ptr, d_ptr, size, cudaMemcpyDeviceToHost);

  YOU manage every transfer. Error-prone but gives full control.

Unified Memory (CUDA 6+):
  cudaMallocManaged(&ptr, size);
  // ptr is valid on BOTH CPU and GPU!

  // CPU writes to it
  ptr[0] = 42;

  // GPU reads it — driver automatically migrates pages
  kernel<<<...>>>(ptr);
  cudaDeviceSynchronize();

  // CPU reads result — pages migrated back automatically
  printf("%d\n", ptr[0]);

How it works under the hood:
  ┌────────────────────────────────────────────────────────────┐
  │ Unified Memory uses PAGE FAULTS (like OS virtual memory):  │
  │                                                            │
  │ 1. GPU kernel accesses address 0x1000                      │
  │ 2. Page not in GPU memory → GPU page fault                 │
  │ 3. Driver migrates page from CPU to GPU over PCIe          │
  │ 4. GPU retries access → succeeds                           │
  │                                                            │
  │ Same in reverse when CPU accesses GPU-resident data.       │
  └────────────────────────────────────────────────────────────┘

  ┌─────────────────────────┬─────────────────────────────────┐
  │ Explicit transfers      │ Unified Memory                   │
  ├─────────────────────────┼─────────────────────────────────┤
  │ Maximum performance     │ Easier to program                │
  │ Full control            │ Automatic migration              │
  │ No page fault overhead  │ Page faults add latency          │
  │ Complex code            │ Simpler code                     │
  │ Used in: production ML  │ Used in: prototyping, some HPC   │
  └─────────────────────────┴─────────────────────────────────┘

  In practice: production systems use explicit transfers.
  Unified memory is useful for prototyping or irregular access patterns
  where you can't predict what data the GPU will need.
```

### NVLink and GPUDirect — Bypassing the CPU

```
Standard path:
  GPU 0 → PCIe → CPU memory → PCIe → GPU 1    (slow, CPU involved)

GPU Peer-to-Peer (P2P) over NVLink:
  GPU 0 → NVLink → GPU 1                       (fast, CPU not involved)
  900 GB/s (H100 NVLink) vs 32 GB/s (PCIe Gen4)

GPUDirect RDMA (network):
  GPU → NVLink/PCIe → InfiniBand NIC → network → remote GPU
  Bypasses CPU entirely! GPU memory → network → remote GPU memory.
  Used in: distributed training (NCCL all-reduce).

GPUDirect Storage:
  GPU → PCIe → NVMe SSD
  Bypasses CPU and system memory entirely! SSD → GPU memory.
  Used in: loading large datasets, checkpoint loading.

  ┌──────────────────────────────────────────────────────────┐
  │                   Data Path Evolution                      │
  │                                                           │
  │  Traditional:  SSD → CPU RAM → PCIe → GPU HBM            │
  │                (two copies, CPU involved)                  │
  │                                                           │
  │  GPUDirect Storage: SSD → PCIe → GPU HBM                 │
  │                (one copy, no CPU involvement)              │
  │                                                           │
  │  GPUDirect RDMA: Remote GPU → InfiniBand → Local GPU      │
  │                (one copy, no CPU involvement)              │
  └──────────────────────────────────────────────────────────┘
```

### Real-World Heterogeneous Application Patterns

```
Pattern 1: ML INFERENCE SERVER
  ┌─────────────────────────────────────────────────────────────┐
  │ CPU threads:                  GPU:                           │
  │  ├── HTTP server (tokio)       Model weights resident in HBM │
  │  ├── Request queue             (loaded once at startup)      │
  │  ├── Tokenizer (CPU-bound)                                   │
  │  ├── Batch assembler          Continuous batching:           │
  │  │   (group requests)          ├── Prefill (new prompts)     │
  │  ├── Detokenizer              ├── Decode (generate tokens)  │
  │  └── Response sender          └── KV-cache management       │
  │                                                               │
  │ CPU is busy ~5% of time. GPU is busy ~90%.                    │
  │ But you STILL need the CPU — it's the orchestrator.           │
  │ Bottleneck: GPU HBM bandwidth (memory-bound for inference).  │
  └─────────────────────────────────────────────────────────────┘

Pattern 2: VIDEO PROCESSING PIPELINE
  ┌─────────────────────────────────────────────────────────────┐
  │ CPU:  Read frames from disk → Decode (sometimes GPU-accel)  │
  │ GPU:  Resize → Color convert → Run detection model          │
  │ CPU:  Post-process detections → Write annotations            │
  │ GPU:  Encode output video (NVENC)                            │
  │                                                               │
  │ Data ping-pongs between CPU and GPU.                         │
  │ Key optimization: keep data on GPU as long as possible.      │
  │ Don't copy to CPU between GPU stages.                        │
  └─────────────────────────────────────────────────────────────┘

Pattern 3: DATABASE WITH GPU ACCELERATION (e.g., RAPIDS, BlazingSQL)
  ┌─────────────────────────────────────────────────────────────┐
  │ CPU:  Parse SQL → Query planning → Orchestration             │
  │ GPU:  Filtering → Joins → Aggregation → Sort                 │
  │ CPU:  Result formatting → Network send                       │
  │                                                               │
  │ GPU is great at: scanning huge tables, hash joins, sorts     │
  │ CPU is still needed for: parsing, planning, networking       │
  │ Transfer cost: load table into GPU once, run many queries.   │
  └─────────────────────────────────────────────────────────────┘

Pattern 4: SIMULATION / HPC
  ┌─────────────────────────────────────────────────────────────┐
  │ CPU:  Initialize state → Set up boundary conditions          │
  │ GPU:  Time-step loop (98% of runtime here):                  │
  │       ├── Compute forces / fluxes (kernel 1)                 │
  │       ├── Update positions / state (kernel 2)                │
  │       └── Halo exchange with neighbor GPUs (NCCL/MPI)        │
  │ CPU:  Periodic checkpointing → Write output files            │
  │                                                               │
  │ GPU does the heavy math. CPU handles I/O and orchestration.  │
  │ Data stays on GPU for thousands of iterations.               │
  │ Multi-GPU: GPUDirect RDMA for halo exchange, no CPU copy.    │
  └─────────────────────────────────────────────────────────────┘
```

### When to Use GPU vs CPU vs Both

```
┌───────────────────────────┬─────────┬───────────────────────────────┐
│ Workload                  │ Run on  │ Why                           │
├───────────────────────────┼─────────┼───────────────────────────────┤
│ Matrix multiply           │ GPU     │ Massively parallel, regular   │
│ ML training / inference   │ GPU     │ Matmul-dominated              │
│ Image/video processing    │ GPU     │ Pixel-parallel, regular       │
│ Monte Carlo simulation    │ GPU     │ Embarrassingly parallel       │
│ Large table scan/filter   │ GPU     │ Data-parallel, high bandwidth │
│                           │         │                               │
│ Web server / API          │ CPU     │ Sequential, I/O-bound, branchy│
│ String processing / regex │ CPU     │ Branchy, irregular            │
│ Tree/graph traversal      │ CPU     │ Pointer-chasing, unpredictable│
│ Compression (general)     │ CPU     │ Complex branching             │
│ Small data (< 1MB)        │ CPU     │ PCIe transfer > compute time  │
│                           │         │                               │
│ ML inference pipeline     │ BOTH    │ CPU: tokenize, batch, decode  │
│                           │         │ GPU: model forward pass       │
│ Video analytics           │ BOTH    │ CPU: decode, post-process     │
│                           │         │ GPU: detect, classify, encode │
│ Database query engine     │ BOTH    │ CPU: parse, plan. GPU: execute│
└───────────────────────────┴─────────┴───────────────────────────────┘

The common mistake:
  "Let's put everything on the GPU, it's faster!"
  No. Moving data to the GPU costs time. If the computation is
  small or branchy, the PCIe transfer alone takes longer than
  just doing it on the CPU.

  Rule: GPU wins when compute_time >> transfer_time.
  If in doubt, profile. Don't guess.
```

## 10. Multi-GPU & Distributed Training

Training a large model on a single GPU is impossible when the model or data
doesn't fit. You must split the work across multiple GPUs — that's distributed
training. There are fundamentally different strategies for how to split.

### Data Parallelism (DP) — Same Model, Split Data

```
The simplest approach. Every GPU has a FULL copy of the model.
Each GPU processes a different mini-batch. Gradients are averaged.

  ┌────────────────────────────────────────────────────────────────┐
  │                    Data Parallelism                             │
  │                                                                │
  │  GPU 0              GPU 1              GPU 2              GPU 3│
  │  ┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐
  │  │ Full model │    │ Full model │    │ Full model │    │ Full model │
  │  │ (copy)     │    │ (copy)     │    │ (copy)     │    │ (copy)     │
  │  ├────────────┤    ├────────────┤    ├────────────┤    ├────────────┤
  │  │ Batch 0    │    │ Batch 1    │    │ Batch 2    │    │ Batch 3    │
  │  └─────┬──────┘    └─────┬──────┘    └─────┬──────┘    └─────┬──────┘
  │        │                 │                 │                 │
  │        └─────── AllReduce (average gradients) ──────────────┘
  │                       via NCCL                                │
  │        ┌─────────────────────────────────────────────┐       │
  │        │  All GPUs now have identical averaged grads  │       │
  │        │  Each GPU updates its own model copy         │       │
  │        └─────────────────────────────────────────────┘       │
  └────────────────────────────────────────────────────────────────┘

  Works: when the model fits on 1 GPU (< 80GB for A100)
  Scales: batch size scales linearly (4 GPUs = 4x batch)
  Bottleneck: AllReduce communication. Gradient size = model size.
    GPT-2 (1.5B params × 4 bytes) = 6 GB of gradients per step.
    Over NVLink (900 GB/s) = ~7 ms. Over InfiniBand (50 GB/s) = ~120 ms.

  NCCL (NVIDIA Collective Communications Library):
    Implements AllReduce, AllGather, ReduceScatter efficiently.
    Uses ring or tree algorithms. Overlaps communication with backward pass.
    PyTorch DDP (DistributedDataParallel) uses NCCL under the hood.
```

### Tensor Parallelism (TP) — Split Layers Across GPUs

```
Split individual layers across GPUs. Each GPU holds a SLICE of each layer.
Used when a single layer's weights don't fit on one GPU.

  Example: a linear layer with weight matrix W (4096 × 16384)

  TP=4 (split across 4 GPUs):
  ┌──────────────────────────────────────────────────────────┐
  │  GPU 0: W[:, 0:4096]      ← columns 0-4095              │
  │  GPU 1: W[:, 4096:8192]   ← columns 4096-8191           │
  │  GPU 2: W[:, 8192:12288]  ← columns 8192-12287          │
  │  GPU 3: W[:, 12288:16384] ← columns 12288-16383         │
  │                                                           │
  │  Each GPU computes its slice: y_i = x @ W_i               │
  │  Then AllGather or ReduceScatter to combine results       │
  │  → requires communication WITHIN every layer forward pass │
  └──────────────────────────────────────────────────────────┘

  Requires: very fast GPU↔GPU communication (NVLink, NOT PCIe/InfiniBand)
  Why: communication happens at EVERY layer, not just at gradient sync.
  Used in: Megatron-LM (NVIDIA), within a single node (8 GPUs).
```

### Pipeline Parallelism (PP) — Split Layers Into Stages

```
Split the model by LAYERS. GPU 0 runs layers 0-9, GPU 1 runs layers 10-19, etc.

  ┌──────────────────────────────────────────────────────────────┐
  │                  Pipeline Parallelism                         │
  │                                                               │
  │  GPU 0           GPU 1           GPU 2           GPU 3       │
  │  Layers 0-9      Layers 10-19    Layers 20-29    Layers 30-39│
  │  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐  │
  │  │ Stage 0 │───►│ Stage 1 │───►│ Stage 2 │───►│ Stage 3 │  │
  │  └─────────┘    └─────────┘    └─────────┘    └─────────┘  │
  │                                                               │
  │  Problem: "pipeline bubble"                                   │
  │  GPU 1 is IDLE while GPU 0 is computing stage 0.            │
  │  GPU 2 is IDLE while GPU 0+1 are computing.                 │
  │  → 75% of GPUs idle during warmup/cooldown                  │
  │                                                               │
  │  Solution: micro-batching (GPipe, PipeDream)                 │
  │  Split batch into micro-batches, pipeline them:              │
  │                                                               │
  │  Time ─────────────────────────────────────────►             │
  │  GPU 0: [μb0][μb1][μb2][μb3]                                │
  │  GPU 1:      [μb0][μb1][μb2][μb3]                           │
  │  GPU 2:           [μb0][μb1][μb2][μb3]                      │
  │  GPU 3:                [μb0][μb1][μb2][μb3]                 │
  │         Less idle time! But still some bubble at edges.      │
  └──────────────────────────────────────────────────────────────┘

  Communication: only activations between stages (one transfer per layer boundary)
  → Works over InfiniBand (cross-node), unlike TP.
```

### How They Combine — Training LLaMA 405B

```
Real large-model training uses ALL THREE together:

  ┌────────────────────────────────────────────────────────────┐
  │  8 nodes × 8 GPUs = 64 GPUs total                          │
  │                                                             │
  │  Tensor Parallelism (TP=8):                                │
  │    Within each node, 8 GPUs split each layer               │
  │    Connected by NVLink (900 GB/s) — fast enough             │
  │                                                             │
  │  Pipeline Parallelism (PP=4):                              │
  │    4 stages across 4 groups of nodes                        │
  │    Each stage holds ~100 layers                             │
  │    Connected by InfiniBand (50 GB/s) — slower but less freq │
  │                                                             │
  │  Data Parallelism (DP=2):                                  │
  │    2 replicas of the full pipeline                          │
  │    Each processes different data, syncs gradients            │
  │                                                             │
  │  Total: TP=8 × PP=4 × DP=2 = 64 GPUs                     │
  │                                                             │
  │  Node 0 (TP group):  [GPU0 GPU1 GPU2 GPU3 GPU4 GPU5 GPU6 GPU7]  Stage 0, Replica 0
  │  Node 1 (TP group):  [GPU0 GPU1 GPU2 GPU3 GPU4 GPU5 GPU6 GPU7]  Stage 1, Replica 0
  │  Node 2 (TP group):  [GPU0 GPU1 GPU2 GPU3 GPU4 GPU5 GPU6 GPU7]  Stage 2, Replica 0
  │  Node 3 (TP group):  [GPU0 GPU1 GPU2 GPU3 GPU4 GPU5 GPU6 GPU7]  Stage 3, Replica 0
  │  Node 4-7: same as 0-3 but Replica 1 (different data)     │
  └────────────────────────────────────────────────────────────┘

Key insight: map parallelism strategy to interconnect speed:
  TP (most communication)   → NVLink (fastest, within node)
  PP (moderate communication) → InfiniBand (cross-node)
  DP (least frequent sync)  → InfiniBand (cross-node, overlapped)
```

## 11. Software Ecosystem — From Python to Silicon

```
┌──────────────────────────────────────────────────────────────────┐
│                     GPU Software Stack                            │
│                                                                   │
│  Your code (Python)                                               │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  PyTorch / JAX / TensorFlow                               │    │
│  │  (high-level: tensors, autograd, model definitions)       │    │
│  └───────────────────┬──────────────────────────────────────┘    │
│                      │                                            │
│  ┌──────────────────▼──────────────────────────────────────┐    │
│  │  torch.compile / XLA / Triton (OpenAI)                    │    │
│  │  (graph optimization, kernel fusion, code generation)     │    │
│  └───────────────────┬──────────────────────────────────────┘    │
│                      │                                            │
│  ┌──────────────────▼──────────────────────────────────────┐    │
│  │  cuBLAS / cuDNN / cuFFT / CUTLASS / FlashAttention       │    │
│  │  (hand-optimized CUDA libraries for specific operations)  │    │
│  └───────────────────┬──────────────────────────────────────┘    │
│                      │                                            │
│  ┌──────────────────▼──────────────────────────────────────┐    │
│  │  CUDA Runtime + CUDA Driver                               │    │
│  │  (memory management, kernel launch, stream scheduling)    │    │
│  └───────────────────┬──────────────────────────────────────┘    │
│                      │                                            │
│  ┌──────────────────▼──────────────────────────────────────┐    │
│  │  GPU Hardware (SMs, Tensor Cores, HBM, NVLink)           │    │
│  └──────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────┘
```

### Key Libraries You Should Know

```
cuBLAS:   Matrix operations (GEMM). Uses Tensor Cores automatically.
          Called by PyTorch for every @ (matmul) operation.

cuDNN:    Neural network primitives (conv, batchnorm, RNN, attention).
          Called by PyTorch for Conv2d, BatchNorm, etc.
          Autotuning: tries multiple algorithms, picks fastest for your shapes.

FlashAttention:
          Fused attention kernel. Tiles Q,K,V in shared memory, avoids
          materializing the N×N attention matrix in HBM.
          Reduces memory from O(N²) to O(N), 2-4x faster than standard attention.
          Used by default in PyTorch 2.0+ (F.scaled_dot_product_attention).

Triton (OpenAI):
          Python DSL for writing GPU kernels. Easier than raw CUDA:
            @triton.jit
            def add_kernel(x_ptr, y_ptr, out_ptr, n, BLOCK: tl.constexpr):
                idx = tl.program_id(0) * BLOCK + tl.arange(0, BLOCK)
                mask = idx < n
                x = tl.load(x_ptr + idx, mask=mask)
                y = tl.load(y_ptr + idx, mask=mask)
                tl.store(out_ptr + idx, x + y, mask=mask)
          torch.compile generates Triton kernels for fused operations.

TensorRT:
          NVIDIA inference optimizer. Takes trained model, applies:
          - Layer fusion (combine conv+bn+relu into one kernel)
          - Precision calibration (FP32 → FP16/INT8 with minimal accuracy loss)
          - Kernel autotuning per GPU type
          Result: 2-5x faster inference vs plain PyTorch.

vLLM / TensorRT-LLM:
          Specialized LLM inference engines:
          - Paged attention (KV-cache memory management like OS virtual memory)
          - Continuous batching (don't wait for longest sequence to finish)
          - Speculative decoding (draft + verify for faster generation)
          - Quantization (FP8, GPTQ, AWQ)
```

### NVIDIA vs The Competition

```
┌──────────────────────┬────────────────┬────────────┬────────────────┐
│                      │ NVIDIA (CUDA)  │ AMD (ROCm) │ Intel (oneAPI) │
├──────────────────────┼────────────────┼────────────┼────────────────┤
│ Market share (ML)    │ ~95%           │ ~4%        │ ~1%            │
│ Mature libraries     │ cuBLAS,cuDNN,  │ rocBLAS,   │ oneMKL,        │
│                      │ NCCL,TensorRT  │ MIOpen     │ oneDNN         │
│ Framework support    │ PyTorch, JAX,  │ PyTorch    │ PyTorch        │
│                      │ TF (native)    │ (works)    │ (basic)        │
│ Key advantage        │ Ecosystem lock │ Price,     │ Gaudi (price)  │
│                      │ 15yr CUDA moat │ open-source│ Integrated GPU │
│ Top chip (2025)      │ B200 (192GB)   │ MI300X     │ Gaudi 3        │
│                      │ 8 TB/s         │ (192GB)    │                │
│ Where used           │ Everywhere     │ Some cloud │ Niche          │
│                      │                │ (Azure)    │                │
└──────────────────────┴────────────────┴────────────┴────────────────┘

The "CUDA moat":
  NVIDIA's real competitive advantage isn't hardware — it's the
  software ecosystem. 15 years of CUDA libraries, tooling, docs,
  developer community, and framework integration.

  Switching from CUDA → ROCm means:
    - Every custom CUDA kernel needs porting
    - Some libraries don't exist (NCCL → RCCL, but less tested)
    - Debugging tools less mature (Nsight vs rocprof)
    - Community support is 10x smaller

  This is why NVIDIA commands premium prices (H100: ~$30K, A100: ~$10K).
```

## 12. MIG & GPU Sharing — Running Multiple Workloads on One GPU

```
A100/H100 are expensive. Running a single small model wastes capacity.
Multi-Instance GPU (MIG) divides one physical GPU into isolated slices.

  ┌─────────────────────────────────────────────────────────────┐
  │                  A100 with MIG enabled                       │
  │                                                              │
  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
  │  │ MIG Instance 0│  │ MIG Instance 1│  │ MIG Instance 2│     │
  │  │ 3g.20gb       │  │ 3g.20gb       │  │ 1g.10gb       │     │
  │  │               │  │               │  │               │     │
  │  │ 28 SMs        │  │ 28 SMs        │  │ 14 SMs        │     │
  │  │ 20 GB HBM     │  │ 20 GB HBM     │  │ 10 GB HBM     │     │
  │  │ Isolated L2   │  │ Isolated L2   │  │ Isolated L2   │     │
  │  │ Own decoders  │  │ Own decoders  │  │ Own decoders  │     │
  │  └──────────────┘  └──────────────┘  └──────────────┘     │
  │                                                              │
  │  Each instance: isolated memory, isolated compute.           │
  │  One instance OOMing does NOT affect the others.             │
  │  Like virtual machines, but at the GPU level.                │
  │  No performance interference (guaranteed SMs and bandwidth). │
  └─────────────────────────────────────────────────────────────┘

  MIG profiles (A100 80GB):
    7 × 1g.10gb  (7 tiny instances, each ~15 SMs, 10 GB)
    3 × 2g.20gb  (3 medium instances)
    2 × 3g.40gb  (2 large instances)
    1 × 7g.80gb  (just the whole GPU)

  Use cases:
    - Multi-tenant inference: 7 small models on 1 GPU
    - CI/CD: give each test job a guaranteed GPU slice
    - Development: engineers share expensive GPUs safely
    - Kubernetes: schedule MIG instances like separate GPUs
```

```
Other GPU sharing approaches:

  ┌────────────────────┬──────────────────────────────────────────┐
  │ Approach           │ How it works                              │
  ├────────────────────┼──────────────────────────────────────────┤
  │ MIG                │ Hardware partitioning. Isolated compute,  │
  │                    │ memory, and bandwidth. No interference.   │
  │                    │ Only A100/H100+. Fixed profiles.          │
  ├────────────────────┼──────────────────────────────────────────┤
  │ MPS (Multi-Process │ Multiple processes share the same GPU.    │
  │ Service)           │ Time-sliced or spatial sharing.           │
  │                    │ Less isolation than MIG. Can interfere.   │
  │                    │ Works on any NVIDIA GPU.                  │
  ├────────────────────┼──────────────────────────────────────────┤
  │ Time-slicing       │ Kubernetes gives pods turn-based access.  │
  │                    │ Simple but wasteful (context switch cost). │
  │                    │ No memory isolation — one pod can OOM all. │
  ├────────────────────┼──────────────────────────────────────────┤
  │ vGPU (NVIDIA GRID) │ Full virtualization for VMs/VDI.          │
  │                    │ Licensed per GPU. Used in enterprise.     │
  │                    │ Not commonly used for ML training.        │
  └────────────────────┴──────────────────────────────────────────┘
```

## 13. GPU Trends & Where Things Are Going

### Hardware Trends

```
1. MEMORY BANDWIDTH IS THE NEW BOTTLENECK
   LLM inference is memory-bound. More compute doesn't help if you
   can't feed the cores fast enough.

   V100 (2017):  900 GB/s
   A100 (2020):  2.0 TB/s    (+122%)
   H100 (2022):  3.35 TB/s   (+67%)
   H200 (2024):  4.8 TB/s    (+43%)
   B200 (2025):  8.0 TB/s    (+67%)

   → HBM bandwidth doubles roughly every 2 generations.
   → Memory CAPACITY also growing: 32→80→80→141→192 GB.
   → This matters because larger models need to fit on fewer GPUs.

2. INTERCONNECT SPEEDS GROWING FASTER THAN COMPUTE
   Multi-GPU communication is the scaling bottleneck.

   NVLink:    300 → 600 → 900 → 1800 GB/s  (doubling per gen)
   InfiniBand: 200 → 400 → 800 Gbps         (doubling per gen)
   Ultra Ethernet: emerging competitor to InfiniBand for AI clusters

   → NVIDIA's "NVLINK SWITCHES" turn 8-GPU nodes into 72-GPU
     "virtual nodes" where every GPU talks to every other at NVLink speed.
     DGX SuperPOD / GB200 NVL72: 72 GPUs acting as one.

3. CHIPLET / MULTI-DIE ARCHITECTURE
   GPUs are following CPUs into chiplet designs:

   B200: two GPU dies connected on one package (MCM — multi-chip module)
   → Doubles transistor count without needing a single enormous die
   → Smaller dies = better yields = lower cost per transistor
   → Same trend as AMD EPYC (CPU chiplets) and Apple M-series (unified)

4. CPU-GPU UNIFICATION
   NVIDIA Grace Hopper (GH200):
     Grace CPU + Hopper GPU on same board
     Connected by NVLink-C2C: 900 GB/s (7x faster than PCIe Gen5)
     Unified memory: CPU and GPU share the same memory pool
     → Eliminates the PCIe bottleneck for heterogeneous workloads
     → GPU can directly access CPU memory at NVLink speed

   Apple already did this: M-series chips have unified memory
   where CPU, GPU, and Neural Engine share the same DRAM pool.
   → No copy needed! But Apple GPUs are weak for ML training.
```

### Software & Architecture Trends

```
5. INFERENCE IS EATING THE WORLD
   Training a model: done once (or a few times).
   Inference: runs millions of times per day.

   Cost breakdown at scale:
     Training GPT-4: ~$100M (one-time)
     Serving GPT-4: ~$700M/year (ongoing, growing)

   → Dedicated inference accelerators emerging:
     - NVIDIA's own split: H100 (train) vs L40S (inference)
     - Google TPU v5e (inference-optimized, cheaper per token)
     - AWS Inferentia/Trainium
     - Groq LPU (deterministic latency, no HBM — all SRAM)
     - Cerebras (wafer-scale chip, entire model in SRAM)

   → Inference optimization techniques matter MORE than raw compute:
     Quantization (FP16→INT8→FP4): 2-4x throughput per precision step
     Speculative decoding: 2-3x faster with draft model
     Continuous batching: 10-20x throughput vs naive batching
     KV-cache compression: fit more sequences in memory

6. SPARSITY AND MIXTURE-OF-EXPERTS (MoE)
   Not all model weights are useful for every input.

   Structured sparsity (H100 Tensor Cores):
     Skip zero-valued weights → 2x speedup with 2:4 sparsity pattern
     (every group of 4 weights has at most 2 non-zero)

   Mixture-of-Experts (Mixtral, GPT-4 rumored):
     Model has 8 expert sub-networks, each input activates only 2.
     → 8x parameters but same compute cost per token.
     → Memory bound! Need to load all experts into memory.
     → MoE models benefit hugely from more HBM capacity.

7. COMPILER-DRIVEN OPTIMIZATION
   Writing hand-optimized CUDA kernels is expensive and brittle.
   The trend: let compilers generate GPU code from high-level descriptions.

   torch.compile:   Python → Triton → CUDA (automatic)
   XLA (JAX/TF):    computation graph → optimized HLO → GPU code
   Triton (OpenAI):  Python-like DSL → efficient CUDA
   MLIR / LLVM:     compiler infrastructure for ML accelerators

   → The "write raw CUDA" era is ending for most developers
   → Hand-tuned CUDA still wins for core kernels (FlashAttention, cuBLAS)
   → But for fused ops and new architectures, compilers are closing the gap

8. THE CLOUD GPU MARKET
   Most companies don't buy GPUs — they rent them.

   ┌─────────────────────┬───────────────────────────────────────┐
   │ Provider            │ GPU Options (2025)                     │
   ├─────────────────────┼───────────────────────────────────────┤
   │ AWS                 │ H100 (p5), Trainium (trn2), Inferentia│
   │ Azure               │ H100, A100, MI300X (AMD!)             │
   │ GCP                 │ H100, A100, TPU v5e/v5p               │
   │ CoreWeave/Lambda    │ H100, B200 (GPU-focused clouds)       │
   │ Together/Fireworks  │ Inference APIs (serverless GPU)        │
   └─────────────────────┴───────────────────────────────────────┘

   Emerging pattern: "GPU as a Service"
     Don't manage hardware. Use inference APIs.
     Only train custom models if you must. Fine-tune otherwise.
     Reserve GPU clusters only for pre-training runs.
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
| "Why not run everything on GPU?" | PCIe transfer (32 GB/s) is 100x slower than GPU memory (3 TB/s). If data is small or computation is branchy, CPU is faster. GPU wins when compute_time >> transfer_time. |
| "How do CPU and GPU work together?" | CPU orchestrates (I/O, tokenization, batching). GPU does parallel compute (matmul, inference). CUDA streams overlap data transfer with compute to hide PCIe latency. |
| "What is GPUDirect?" | Bypass CPU entirely: GPU↔GPU over NVLink (900 GB/s), GPU↔NIC over RDMA (distributed training), GPU↔SSD over Storage (dataset loading). |
| "Data vs tensor vs pipeline parallelism?" | DP: same model on each GPU, split data, sync gradients (AllReduce). TP: split each layer across GPUs (needs NVLink). PP: split by layer groups, pipeline micro-batches. Real training uses all three: TP within node, PP across nodes, DP for replicas. |
| "What is MIG?" | Multi-Instance GPU. Hardware-partitions A100/H100 into isolated instances (own SMs, memory, L2). No interference. Use for multi-tenant inference or shared dev GPUs in K8s. |
| "Why is inference harder than training?" | Training is compute-bound (reuse weights across batch). Inference is memory-bound (load all weights for each token, often batch=1). Inference also has latency SLOs. Requires different optimizations: quantization, KV-cache, continuous batching, speculative decoding. |
| "NVIDIA's moat?" | 15-year CUDA ecosystem: cuBLAS, cuDNN, NCCL, TensorRT, plus framework integration and community. Porting to AMD ROCm means rewriting kernels, less tooling, smaller community. Hardware is good but software keeps customers locked in. |
| "What is FlashAttention?" | Fused attention kernel that tiles Q,K,V in shared memory, avoids materializing N×N attention matrix in HBM. Reduces memory O(N²)→O(N), 2-4x faster. Default in PyTorch 2.0+. |

## Further Reading

- Cornell GPU Architecture Course: https://cvw.cac.cornell.edu/gpu-architecture
