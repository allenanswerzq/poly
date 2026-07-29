# Triton — Write GPU Kernels in Python

---

## 1. What Triton Is

```
Triton is a LANGUAGE + COMPILER for writing GPU kernels in Python.
Instead of writing 500 lines of CUDA with manual thread/warp/shmem
management, you write ~30 lines of Triton and the compiler handles
the low-level details.

  The key abstraction: you think in TILES, not threads.

  CUDA: "I am thread 47 of warp 1 in block 3. I load one element
         from shared memory at bank 15, do my multiply, then
         shuffle my result to thread 48."

  Triton: "I am block 3. I load a 128×64 TILE from memory,
           do a matmul on the whole tile, store the result."
           The compiler figures out threads, warps, shared memory.

  Created by: Philippe Tillet (Harvard PhD → OpenAI).
  Now maintained as part of the PyTorch ecosystem.
  GitHub: https://github.com/triton-lang/triton

  Used by:
    - PyTorch Inductor (torch.compile generates Triton kernels)
    - Flash Attention (Triton implementation)
    - vLLM, SGLang (PagedAttention kernels)
    - TokenSpeed, FlashInfer (custom attention kernels)
    - Thousands of ML research projects

  Why it matters:
    Before Triton, only ~100 people in the world could write
    competitive GPU kernels (CUDA experts at NVIDIA, Meta, Google).
    Triton makes it accessible to any ML researcher who knows Python.
```

---

## 2. The Programming Model — Tiles, Not Threads

```
A Triton kernel processes one BLOCK (tile) of the output.
The runtime launches many blocks in parallel across the GPU.

  Example: vector addition (simplest possible kernel)

    @triton.jit
    def add_kernel(X, Y, Z, N, BLOCK: tl.constexpr):
        pid = tl.program_id(0)               # which block am I?
        offsets = pid * BLOCK + tl.arange(0, BLOCK)  # my indices
        mask = offsets < N                    # bounds check
        x = tl.load(X + offsets, mask=mask)   # load tile from HBM
        y = tl.load(Y + offsets, mask=mask)
        tl.store(Z + offsets, x + y, mask=mask)  # write result

  What the PROGRAMMER specifies:
    - Block size (BLOCK = 1024)
    - What to load, compute, store
    - Bounds masking

  What the COMPILER handles:
    - Thread assignment within the block
    - Shared memory allocation and bank-conflict avoidance
    - Register allocation
    - Memory coalescing (adjacent threads access adjacent memory)
    - Warp-level optimizations
    - Instruction scheduling

  This is the core insight: the tile abstraction is high enough
  to be simple, but structured enough for the compiler to optimize.
```

---

## 3. A Real Kernel — Fused Matmul + ReLU

```
This is where Triton shines: fusing operations that would be
separate kernels in PyTorch.

  @triton.jit
  def matmul_relu_kernel(
      A, B, C,
      M, N, K,
      stride_am, stride_ak,
      stride_bk, stride_bn,
      stride_cm, stride_cn,
      BLOCK_M: tl.constexpr,    # tile rows
      BLOCK_N: tl.constexpr,    # tile cols
      BLOCK_K: tl.constexpr,    # reduction tile
  ):
      pid_m = tl.program_id(0)      # which row-block
      pid_n = tl.program_id(1)      # which col-block

      # Pointers to the tiles of A and B
      offs_m = pid_m * BLOCK_M + tl.arange(0, BLOCK_M)
      offs_n = pid_n * BLOCK_N + tl.arange(0, BLOCK_N)
      offs_k = tl.arange(0, BLOCK_K)

      # Initialize accumulator (stays in registers)
      acc = tl.zeros((BLOCK_M, BLOCK_N), dtype=tl.float32)

      # Loop over K dimension in tiles
      for k in range(0, K, BLOCK_K):
          a = tl.load(A + offs_m[:, None] * stride_am
                        + (k + offs_k[None, :]) * stride_ak)
          b = tl.load(B + (k + offs_k[:, None]) * stride_bk
                        + offs_n[None, :] * stride_bn)
          acc += tl.dot(a, b)         # matmul in SRAM (tensor cores)

      # Fused ReLU — still in registers, no HBM write between
      acc = tl.maximum(acc, 0.0)

      # Write result to HBM (ONE write)
      tl.store(C + offs_m[:, None] * stride_cm
                 + offs_n[None, :] * stride_cn, acc)


  What happens under the hood:

    ┌──────────────────────────────────────────────────────────┐
    │ For each block (pid_m, pid_n):                          │
    │                                                          │
    │   1. Load BLOCK_M × BLOCK_K tile of A from HBM → SRAM  │
    │   2. Load BLOCK_K × BLOCK_N tile of B from HBM → SRAM  │
    │   3. Multiply in SRAM (uses tensor cores if available)   │
    │   4. Accumulate in registers (FP32)                      │
    │   5. Repeat for next K-tile                              │
    │   6. After all K-tiles: apply ReLU (still in registers!) │
    │   7. Write BLOCK_M × BLOCK_N result to HBM              │
    │                                                          │
    │   Matmul + ReLU: ONE read of A, ONE read of B,          │
    │   ONE write of C. Zero intermediate HBM traffic.         │
    └──────────────────────────────────────────────────────────┘

  Equivalent in PyTorch (unfused):
    temp = A @ B        # GEMM kernel: read A,B → write temp to HBM
    C = relu(temp)      # ReLU kernel: read temp from HBM → write C
    → 2× the HBM traffic for the intermediate tensor.
```

---

## 4. The Compilation Pipeline

```
Triton kernel → GPU machine code in four stages:

  ┌─────────────────────────────────────────────────────────────┐
  │ @triton.jit                                                 │
  │ def my_kernel(...)                                          │
  └───────────────────────────┬─────────────────────────────────┘
                              │ Python AST → Triton IR
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Triton IR (MLIR-based, "Triton dialect")                    │
  │                                                             │
  │   Represents tiles as first-class objects.                  │
  │   Operations: tt.load, tt.store, tt.dot, tt.reduce, etc.   │
  │   Still hardware-agnostic at this level.                    │
  └───────────────────────────┬─────────────────────────────────┘
                              │ Triton → TritonGPU dialect
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ TritonGPU IR                                                │
  │                                                             │
  │   Tiles are now mapped to:                                  │
  │   - Warps: how tiles are distributed across warps           │
  │   - Shared memory: which tiles go through SRAM              │
  │   - Tensor cores: which dot ops use HMMA instructions       │
  │                                                             │
  │   Optimization passes:                                      │
  │   - Pipeline: overlap memory loads with compute             │
  │   - Coalesce: ensure adjacent threads access adjacent memory│
  │   - Swizzle: avoid shared memory bank conflicts             │
  │   - Prefetch: software pipelining of memory loads           │
  └───────────────────────────┬─────────────────────────────────┘
                              │ Lower to LLVM IR
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ LLVM IR → PTX → SASS                                       │
  │                                                             │
  │   LLVM IR: standard LLVM with NVPTX target                 │
  │   PTX: NVIDIA virtual assembly (portable across GPU gens)  │
  │   SASS: actual GPU machine code (architecture-specific)     │
  │                                                             │
  │   For AMD: LLVM IR → AMDGPU → GFX assembly                │
  └─────────────────────────────────────────────────────────────┘

  The whole pipeline uses MLIR internally (Triton dialect → LLVM dialect).
  This is why Triton can target multiple backends with shared infrastructure.
```

---

## 5. Auto-Tuning — Let the Compiler Find the Best Config

```
Tile sizes dramatically affect performance.
Wrong tile sizes → 2-5× slower than optimal.

  Example: matmul of [4096 × 4096] × [4096 × 4096]
    BLOCK_M=32,  BLOCK_N=32,  BLOCK_K=32:   slow (tiles too small)
    BLOCK_M=128, BLOCK_N=128, BLOCK_K=32:   fast (good for A100)
    BLOCK_M=256, BLOCK_N=64,  BLOCK_K=64:   fastest on this shape
    BLOCK_M=128, BLOCK_N=256, BLOCK_K=64:   fastest on different GPU

  Triton's auto-tuning:

    @triton.autotune(
        configs=[
            triton.Config({'BLOCK_M': 128, 'BLOCK_N': 128, 'BLOCK_K': 32},
                          num_warps=4, num_stages=3),
            triton.Config({'BLOCK_M': 256, 'BLOCK_N': 64,  'BLOCK_K': 64},
                          num_warps=8, num_stages=3),
            triton.Config({'BLOCK_M': 64,  'BLOCK_N': 256, 'BLOCK_K': 64},
                          num_warps=8, num_stages=4),
            # ... 10-20 configs
        ],
        key=['M', 'N', 'K'],  # re-tune when problem size changes
    )
    @triton.jit
    def matmul_kernel(...):
        ...

  First call: Triton benchmarks ALL configs on actual hardware.
  Picks the fastest. Caches the result.
  Subsequent calls: uses the cached winner.

  The configs control:
    BLOCK_M/N/K:  tile dimensions
    num_warps:    how many warps per block (4 or 8 typically)
    num_stages:   software pipeline depth (overlap loads with compute)
```

---

## 6. How torch.compile Uses Triton (Inductor)

```
When you write @torch.compile, PyTorch's Inductor backend
GENERATES Triton kernels automatically.

  @torch.compile
  def f(x, w, b):
      return F.gelu(x @ w + b)

  Inductor sees: matmul → add → gelu
    matmul: call cuBLAS (vendor library is faster for large GEMMs)
    add + gelu: GENERATE a Triton kernel that fuses them

  Generated Triton kernel (simplified):
    @triton.jit
    def fused_add_gelu(out_ptr, bias_ptr, N, BLOCK: tl.constexpr):
        pid = tl.program_id(0)
        offs = pid * BLOCK + tl.arange(0, BLOCK)
        x = tl.load(out_ptr + offs)     # load matmul result
        b = tl.load(bias_ptr + offs)
        x = x + b                        # add bias
        # GELU approximation
        x = 0.5 * x * (1 + tl.math.tanh(0.7978845 * (x + 0.044715 * x * x * x)))
        tl.store(out_ptr + offs, x)      # write in-place

  The decision logic:
    ┌─────────────────────────────────────────────────────────┐
    │ Operation type        │ Inductor's choice              │
    ├───────────────────────┼────────────────────────────────┤
    │ Large matmul          │ cuBLAS (hand-tuned, fastest)   │
    │ Convolution           │ cuDNN (hand-tuned)             │
    │ Attention             │ Flash Attention / FlexAttention │
    │ Elementwise chain     │ Generate fused Triton kernel   │
    │ Reduction (sum, mean) │ Generate Triton kernel         │
    │ Custom op             │ Generate Triton kernel         │
    └───────────────────────┴────────────────────────────────┘

  Inductor generates Triton for everything that ISN'T a GEMM or conv.
  For GEMM/conv, vendor libraries (cuBLAS/cuDNN) are still faster
  because they use NVIDIA-internal optimizations Triton can't access.
```

---

## 7. Flash Attention in Triton

```
The canonical example of what Triton enables:
  Flash Attention in ~200 lines of Triton vs ~2000 lines of CUDA.

  Key idea: fuse Q @ K^T → softmax → @ V into ONE kernel,
  computing in tiles that fit in SRAM.

  @triton.jit
  def flash_attn_fwd(Q, K, V, Out, ...):
      # Each block handles one tile of Q (rows)
      q_block = tl.load(Q + q_offsets)    # load Q tile to SRAM

      m_i = float('-inf')                  # running max (for softmax)
      l_i = 0.0                            # running sum (for softmax)
      acc = tl.zeros(...)                  # output accumulator

      for j in range(num_kv_blocks):
          k_block = tl.load(K + k_offsets) # load K tile
          v_block = tl.load(V + v_offsets) # load V tile

          # Compute attention scores in SRAM
          scores = tl.dot(q_block, tl.trans(k_block))
          scores *= scale

          # Online softmax (the key trick)
          m_new = tl.maximum(m_i, tl.max(scores, axis=1))
          p = tl.exp(scores - m_new[:, None])
          l_new = tl.exp(m_i - m_new) * l_i + tl.sum(p, axis=1)

          # Rescale old accumulation + add new
          acc = acc * (tl.exp(m_i - m_new) * l_i / l_new)[:, None]
          acc += tl.dot(p, v_block) / l_new[:, None]

          m_i = m_new
          l_i = l_new

      tl.store(Out + out_offsets, acc)     # ONE write to HBM

  Why this is hard in CUDA but natural in Triton:
    CUDA: manage warps loading KV tiles, shared memory layout,
          bank conflicts, warp-level reductions for softmax,
          register pressure from accumulator. ~2000 lines.
    Triton: tl.load, tl.dot, tl.exp, tl.store on tile-shaped data.
          Compiler handles all the warp/shmem/register details.
          ~200 lines.
```

---

## 8. Triton vs CUDA — When to Use Each

```
  ┌──────────────────────────────────────────────────────────────┐
  │              │ Triton                │ CUDA                  │
  ├──────────────┼───────────────────────┼───────────────────────┤
  │ Language     │ Python-like DSL       │ C++ with extensions   │
  │ Abstraction  │ Tiles (blocks)        │ Threads + warps       │
  │ Shmem mgmt   │ Automatic             │ Manual                │
  │ Performance  │ 80-95% of hand-tuned  │ 100% (by definition)  │
  │ Code length  │ ~30 lines per kernel  │ ~300-500 lines        │
  │ Debug        │ Python-level errors   │ CUDA-gdb, Nsight      │
  │ Auto-tune    │ Built-in              │ Manual / cuBLAS       │
  │ Learning     │ Days                  │ Months                │
  │ Portability  │ NVIDIA + AMD (triton) │ NVIDIA only           │
  │ Best for     │ Fused ops, research,  │ Max perf, matmul,     │
  │              │ most custom kernels   │ vendor libraries      │
  └──────────────┴───────────────────────┴───────────────────────┘

  Where CUDA still wins:
    1. Large GEMMs: cuBLAS uses NVIDIA-internal instructions
       (HMMA warp-level, async copy, TMA) that Triton can't
       fully exploit. Gap: ~5-15% for large matmul.
    2. Absolute peak performance kernels where every register,
       every instruction matters (e.g., NCCL AllReduce).
    3. Warp-level primitives: shuffle, vote, ballot — Triton
       abstracts these away, which means you can't use them.

  Where Triton wins:
    1. Fused elementwise: Triton fuses any chain of pointwise ops.
       Writing the same in CUDA is tedious and error-prone.
    2. Rapid iteration: change tile size, re-run. No recompile.
    3. Research: try a new attention variant in an afternoon,
       not a week of CUDA debugging.
    4. Portability: same kernel runs on NVIDIA + AMD.
```

---

## 9. Triton for AMD (triton-amd / ROCm)

```
Triton was originally NVIDIA-only.
AMD invested heavily in a Triton backend for ROCm:

  Triton IR → TritonGPU IR → LLVM IR → AMDGPU backend → GFX ISA

  Same Triton kernel source code → runs on AMD GPUs.
  This is a huge deal: vLLM, SGLang, Flash Attention all use
  Triton kernels. AMD support means the inference ecosystem
  works on MI300X without rewriting kernels in HIP/ROCm.

  Status (2026):
    - Core operations work well (elementwise, reduction, matmul)
    - Flash Attention Triton variant works on MI300X
    - Performance gap vs NVIDIA still exists (~10-20% on some kernels)
    - Actively developed by AMD + community

  The Triton backend architecture makes this possible:
    Frontend (Python → Triton IR): shared, hardware-agnostic
    Backend (Triton IR → machine code): pluggable per target
    Adding a new target = writing a new backend, not rewriting kernels.
```

---

## 10. Key Numbers

```
Triton performance vs alternatives (A100, representative):

  Fused elementwise (add + gelu + dropout, 8M elements):
    PyTorch eager:    1.0× (3 separate kernels)
    Triton fused:     2.5× (1 kernel, 3× less HBM traffic)
    CUDA hand-tuned:  2.7× (marginal improvement over Triton)

  Matrix multiply ([4096 × 4096] × [4096 × 4096], FP16):
    cuBLAS:           1.0× (the gold standard)
    Triton matmul:    0.85-0.95× (close but not quite)
    PyTorch eager:    1.0× (just calls cuBLAS)

  Flash Attention (seq=4096, heads=32, d=128, FP16):
    Triton FA:        1.0× (reference)
    CUDA FA (Dao):    1.1-1.2× (hand-tuned, slightly faster)
    PyTorch eager:    0.3× (3-4× slower, O(N²) memory)

  Compilation time:
    Simple kernel (add):       ~0.5 seconds (first call)
    Complex kernel (matmul):   ~2-5 seconds (first call)
    With auto-tune (10 configs): ~10-30 seconds
    Cached (subsequent calls):  <1 ms

  Lines of code (approximate):
    Vector add:     Triton ~10 lines, CUDA ~30 lines
    Matmul:         Triton ~40 lines, CUDA ~500 lines
    Flash Attention: Triton ~200 lines, CUDA ~2000 lines
    Fused Adam:     Triton ~50 lines, CUDA ~200 lines
```
