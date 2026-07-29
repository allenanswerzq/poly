# XLA (Accelerated Linear Algebra) — Google's ML Compiler

---

## 1. What XLA Is

```
XLA is a WHOLE-GRAPH compiler for machine learning.
Give it a computation graph (matmul, conv, elementwise ops),
it optimizes everything globally and generates GPU/TPU/CPU code.

  The key difference from Triton:
    Triton: you write ONE kernel at a time.
    XLA: you give it the ENTIRE model, it optimizes across ALL ops.
    Global view → better fusion, better memory planning.

  Created by: Google (2017+, publicly part of TensorFlow).
  The ONLY compiler that targets Google TPUs effectively.
  Also works on NVIDIA GPUs and CPUs.

  Used by:
    - JAX (PRIMARY backend — JAX barely works without XLA)
    - TensorFlow (optional, required for TPU)
    - PyTorch via torch-xla (for running PyTorch on TPUs)

  Why it matters:
    XLA powers ALL of Google's AI: Gemini, PaLM, BERT, T5,
    AlphaFold, everything. When Google trains on TPU pods,
    XLA is the compiler making it run.
```

---

## 2. How XLA Compiles a Model

```
The pipeline:

  ┌─────────────────────────────────────────────────────────────┐
  │ Step 1: CAPTURE — Framework produces HLO                    │
  │                                                             │
  │   JAX:                                                      │
  │     jax.jit(f)(x) → JAX traces f → produces Jaxpr IR       │
  │     Jaxpr → StableHLO (portable MLIR dialect)              │
  │     StableHLO → HLO (XLA's native IR)                     │
  │                                                             │
  │   TensorFlow:                                               │
  │     @tf.function → TF graph → XLA bridge → HLO            │
  │                                                             │
  │   PyTorch (torch-xla):                                     │
  │     Lazy tensor tracing → HLO                              │
  └───────────────────────────┬─────────────────────────────────┘
                              │
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Step 2: HLO OPTIMIZATION PASSES (the core of XLA)          │
  │                                                             │
  │   The HLO graph goes through 50+ optimization passes:      │
  │                                                             │
  │   Algebraic simplification:                                │
  │     x * 1 → x                                             │
  │     x + 0 → x                                             │
  │     transpose(transpose(x)) → x                           │
  │     reshape(reshape(x, s1), s2) → reshape(x, s2)          │
  │                                                             │
  │   Operator fusion (THE most important pass):               │
  │     See section 3 below.                                   │
  │                                                             │
  │   Layout optimization:                                     │
  │     Choose row-major vs column-major vs tiled layout       │
  │     for EACH tensor to minimize transposes/copies.         │
  │     Different on GPU (row-major) vs TPU (tiled 128×128).   │
  │                                                             │
  │   Buffer assignment:                                       │
  │     Tensors A and B never alive at the same time?          │
  │     → Share the same memory buffer. Reduces peak memory.    │
  │                                                             │
  │   Common subexpression elimination (CSE):                  │
  │     If the same computation appears twice, do it once.     │
  │                                                             │
  │   Constant folding:                                        │
  │     Anything computable at compile time → precompute.      │
  │                                                             │
  │   Dead code elimination:                                   │
  │     Remove computations whose outputs are never used.       │
  │                                                             │
  │   While loop optimization:                                 │
  │     Loop-invariant code motion, loop unrolling.            │
  └───────────────────────────┬─────────────────────────────────┘
                              │
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Step 3: CODE GENERATION                                     │
  │                                                             │
  │   For NVIDIA GPU:                                           │
  │     HLO → LLVM IR (with NVPTX target)                     │
  │     → PTX → SASS (GPU machine code)                        │
  │     Uses cuBLAS/cuDNN for GEMM/conv when beneficial.       │
  │                                                             │
  │   For Google TPU:                                           │
  │     HLO → TPU-specific instructions                        │
  │     TPU has a different ISA (systolic array + vector unit). │
  │     XLA knows the TPU's 128×128 matrix unit intimately.    │
  │                                                             │
  │   For CPU:                                                  │
  │     HLO → LLVM IR → x86/ARM machine code                  │
  │     LLVM handles vectorization (AVX-512, NEON).            │
  └─────────────────────────────────────────────────────────────┘
```

---

## 3. Operator Fusion — XLA's Superpower

```
XLA sees the ENTIRE graph and can fuse aggressively.

  Fusion categories in XLA:

  1. ELEMENTWISE FUSION (kLoop)
     Chain of elementwise ops → one kernel.
       add → multiply → exp → add → ...
     All use the same loop structure, combine trivially.

  2. REDUCTION FUSION (kInput)
     Elementwise ops feeding into a reduction:
       x → exp → sum → divide
     Fused: compute exp and accumulate sum in one pass.

  3. TRANSPOSE FUSION
     Transpose + elementwise: fold the transpose into the
     memory access pattern. No separate transpose kernel.

  4. DOT FUSION (matmul + epilogue)
     Matmul → bias → activation → residual in one kernel.
     Similar to TensorRT's epilogue fusion.

  Why XLA fuses BETTER than kernel-level approaches:

    Triton/CUDA: you write each kernel. Fusion is manual.
    Inductor: fuses within a subgraph but has graph breaks.
    XLA: sees EVERYTHING. Can fuse across what would be
         separate subgraphs in other compilers.

    Example: LayerNorm × 2 + residual + dropout
      PyTorch eager: 6+ kernels
      Inductor:      2-3 kernels (some fusion)
      XLA:           1 kernel (fuses entire chain)

  The tradeoff:
    More fusion → better performance
    But: requires static shapes to plan fusion at compile time.
    This is why XLA needs to know all tensor shapes upfront.
```

---

## 4. HLO — The Intermediate Representation

```
HLO (High-Level Operations) is XLA's IR. Everything goes through it.

  HLO operation types (subset):
    dot           → matmul
    convolution   → convolution (very general, subsumes many variants)
    reduce        → sum, max, min, etc. along axes
    broadcast     → expand tensor dimensions
    transpose     → reorder dimensions
    reshape       → change shape without data movement
    slice         → extract sub-tensor
    concatenate   → join tensors
    add/mul/etc   → elementwise arithmetic
    exp/log/tanh  → elementwise transcendentals
    select        → conditional elementwise (where)
    scatter/gather → indexed writes/reads
    while         → loops
    conditional   → if/else
    all-reduce    → distributed communication
    custom-call   → escape hatch (call external kernels)

  Example HLO for y = relu(x @ W + b):

    p0 = f32[32,784]{1,0} parameter(0)         # x
    p1 = f32[784,256]{1,0} parameter(1)         # W
    p2 = f32[256]{0} parameter(2)               # b
    dot = f32[32,256]{1,0} dot(p0, p1)          # x @ W
    broadcast = f32[32,256]{1,0} broadcast(p2)  # expand b
    add = f32[32,256]{1,0} add(dot, broadcast)  # + b
    zero = f32[] constant(0)
    bcast_zero = f32[32,256]{1,0} broadcast(zero)
    ROOT relu = f32[32,256]{1,0} maximum(add, bcast_zero)  # relu

  The {1,0} annotations are LAYOUT: dimension ordering in memory.
  XLA optimizes this per-op to minimize data movement.

StableHLO:
  Portable version of HLO, defined as an MLIR dialect.
  JAX produces StableHLO → XLA consumes it.
  Other compilers can also consume StableHLO.
  Goal: decouple JAX from XLA's internal HLO format,
  so other backends (IREE, etc.) can process the same IR.
```

---

## 5. XLA for TPUs — Why It's Unique

```
TPUs have a fundamentally different architecture from GPUs:

  GPU (NVIDIA H100):
    - 18,432 CUDA cores + 528 Tensor Cores
    - FLEXIBLE: any thread can do any work
    - Memory: HBM + shared memory (SRAM) + registers
    - ISA: general-purpose PTX/SASS instructions

  TPU v5e:
    - 2D systolic array: 128×128 matrix multiply unit
    - SPECIALIZED: efficient for regular, tiled computation
    - Memory: HBM + 128 MB SRAM (much larger than GPU SRAM)
    - ISA: custom, vector + matrix instructions

  XLA for TPU must:
    1. Tile all computations to 128×128 blocks (systolic array size)
    2. Use TPU's large SRAM (128 MB vs GPU's ~20 MB)
       → can keep much more data on-chip → less HBM traffic
    3. Schedule vector unit + matrix unit in parallel
       (TPU can do elementwise on vector unit while matrix unit
        does matmul)
    4. Handle TPU's inter-chip interconnect (ICI)
       for multi-TPU parallelism

  ┌──────────────────────────────────────────────────────────┐
  │ TPU v5e compute:                                         │
  │                                                          │
  │   ┌─────────────────┐  ┌──────────────────┐             │
  │   │  Matrix Unit    │  │  Vector Unit     │             │
  │   │  128×128        │  │  (elementwise)   │             │
  │   │  systolic array │  │                  │             │
  │   │  BF16: 197 TF   │  │  Runs in parallel│             │
  │   └────────┬────────┘  └────────┬─────────┘             │
  │            │                    │                        │
  │            └────────┬───────────┘                        │
  │                     ▼                                    │
  │            ┌────────────────┐                            │
  │            │  128 MB SRAM   │  ← 10× more than GPU!     │
  │            │  (on-chip)     │                            │
  │            └────────┬───────┘                            │
  │                     ▼                                    │
  │            ┌────────────────┐                            │
  │            │  16-32 GB HBM  │                            │
  │            └────────────────┘                            │
  └──────────────────────────────────────────────────────────┘

  XLA is the ONLY compiler that knows how to target this.
  No other ML compiler (Triton, Inductor, TVM) supports TPUs.
  This makes XLA + JAX the only serious path for TPU training.
```

---

## 6. GSPMD — Automatic Parallelism

```
GSPMD (General and Scalable Parallelization for ML Graphs)
is XLA's approach to multi-device parallelism.

  The idea: annotate WHAT to shard, let XLA figure out HOW.

    # JAX: specify that weight is sharded across 8 TPUs
    mesh = Mesh(jax.devices(), ('data', 'model'))
    W = jax.device_put(W, NamedSharding(mesh, P(None, 'model')))
    # W's columns are split across 'model' axis devices

    # XLA sees the annotation and AUTOMATICALLY inserts:
    #   AllGather before ops that need the full tensor
    #   ReduceScatter after ops that produce partial results
    #   AllReduce where needed for correctness

  Compare with PyTorch (manual parallelism):
    # Megatron-LM: manually write column-parallel, row-parallel,
    # manually insert AllReduce. 100s of lines of comms code.

  Compare with XLA (automatic):
    # Annotate: split these axes across these devices.
    # XLA inserts all communication. You write nothing.

  GSPMD supported strategies:
    Data parallelism:   shard batch dimension
    Tensor parallelism: shard weight columns/rows
    Pipeline parallel:  shard by layer groups
    Expert parallel:    shard MoE experts across devices
    Any combination:    XLA handles mixed strategies

  This is the single biggest advantage of JAX+XLA for large-scale
  training. Parallelism strategies that take weeks in PyTorch/Megatron
  can be expressed in a few lines of sharding annotations.

  Used by: Google Gemini training, PaLM, and all Google-scale models.
```

---

## 7. Static Shapes — XLA's Major Constraint

```
XLA requires ALL tensor shapes known at compile time.

  Why: fusion, layout, buffer assignment all depend on shapes.
    If shape changes → must recompile. Recompilation: seconds to minutes.

  Problem for LLMs:
    Batch size varies (1 during off-peak, 64 during peak).
    Sequence length varies (each prompt is different length).
    Every new shape triggers recompilation.

  Workarounds:
    1. PADDING: pad all sequences to a fixed length.
       Wastes compute but avoids recompilation.
       Seq lengths [47, 102, 89] → all padded to 128.

    2. BUCKETING: pre-compile for a few fixed shapes.
       Bucket 1: seq ≤ 128
       Bucket 2: seq ≤ 512
       Bucket 3: seq ≤ 2048
       Bucket 4: seq ≤ 8192
       Incoming requests go to the nearest bucket.

    3. DYNAMIC SHAPES (recent, limited):
       XLA now supports some bounded dynamic shapes.
       Shape can vary within a pre-declared range.
       Still less flexible than PyTorch.

  This is the main reason vLLM/SGLang use PyTorch, not JAX:
    Inference workloads have highly variable request shapes.
    Recompilation overhead is unacceptable for serving.

  For TRAINING, static shapes are less of a problem:
    Batch size is fixed. Sequence length is usually fixed or padded.
    Compile once, run millions of steps. Compilation cost amortized.
```

---

## 8. XLA vs Inductor (torch.compile)

```
  ┌──────────────────────────────────────────────────────────────┐
  │                │ XLA                    │ Inductor             │
  ├────────────────┼────────────────────────┼──────────────────────┤
  │ Scope          │ Whole graph (global)   │ Subgraph (per-break) │
  │ Shapes         │ Static required        │ Dynamic supported    │
  │ Framework      │ JAX (native), TF, PT   │ PyTorch only         │
  │ TPU            │ Yes (only option)       │ No                   │
  │ GPU            │ Yes (good, not best)   │ Yes (Triton + cuBLAS)│
  │ Fusion         │ Very aggressive        │ Good (elementwise)   │
  │ Compile time   │ Minutes (first time)   │ Seconds (first time) │
  │ Eager fallback │ No (must compile all)  │ Yes (graph breaks)   │
  │ Parallelism    │ GSPMD (automatic)      │ Manual (DDP/FSDP)    │
  │ Maturity       │ Very high (2017+)      │ Medium (2022+)       │
  │ Debugging      │ Hard (compiled graph)  │ Easier (graph breaks)│
  ├────────────────┼────────────────────────┼──────────────────────┤
  │ Best for       │ TPU training,          │ PyTorch users,       │
  │                │ JAX users, Google-scale│ GPU training + infer │
  └────────────────┴────────────────────────┴──────────────────────┘

  Performance comparison (NVIDIA GPU, same model):
    XLA and Inductor are roughly comparable on GPU.
    XLA sometimes wins on fusion-heavy workloads.
    Inductor sometimes wins by using cuBLAS more effectively.
    Neither dominates the other on GPU.

    On TPU: XLA is the ONLY option. No comparison.
```

---

## 9. XLA in Practice — JAX Example

```
  import jax
  import jax.numpy as jnp
  from jax import jit

  # Define model function (pure function — no side effects)
  def transformer_layer(x, w_qkv, w_out, w_mlp1, w_mlp2):
      # Self-attention
      qkv = x @ w_qkv                          # fused QKV projection
      q, k, v = jnp.split(qkv, 3, axis=-1)
      scores = (q @ k.T) / jnp.sqrt(d)
      attn = jax.nn.softmax(scores) @ v
      x = x + attn @ w_out                      # residual

      # MLP
      h = jax.nn.gelu(x @ w_mlp1)
      x = x + h @ w_mlp2                        # residual
      return x

  # JIT compile: JAX traces → StableHLO → XLA → GPU/TPU code
  fast_layer = jit(transformer_layer)

  # First call: compiles (seconds to minutes)
  out = fast_layer(x, w_qkv, w_out, w_mlp1, w_mlp2)

  # Subsequent calls: runs compiled code (fast)
  out = fast_layer(x, w_qkv, w_out, w_mlp1, w_mlp2)

  # Multi-device: add sharding annotations
  from jax.sharding import Mesh, NamedSharding, PartitionSpec as P

  mesh = Mesh(jax.devices(), ('dp', 'tp'))
  # Shard weights across tensor-parallel axis
  w_qkv = jax.device_put(w_qkv, NamedSharding(mesh, P(None, 'tp')))
  # XLA/GSPMD auto-inserts AllReduce, AllGather as needed

  The JAX philosophy:
    Write pure functions → jit → XLA compiles → fast execution.
    Parallelism via sharding annotations → GSPMD handles communication.
    No manual kernel writing. No manual communication code.
    Tradeoff: must live within XLA's constraints (static shapes, etc.).
```

---

## 10. Key Numbers

```
XLA compilation and performance:

  Compilation time (first call):
    Small model (ResNet-50):  ~5-10 seconds
    Medium (BERT-large):      ~30-60 seconds
    Large (LLaMA-70B):       ~3-10 minutes
    Massive (PaLM-540B):     ~15-30 minutes
    Cached after first compile → <1 second subsequent runs.

  Performance vs PyTorch eager (NVIDIA A100):
    Transformer training:  1.3-1.6× faster (fusion + optimization)
    CNN training:          1.2-1.5× faster
    Inference:             1.5-2.5× faster (static graphs, full fusion)

  TPU performance (where XLA shines):
    TPU v5e BF16 peak:    197 TFLOPS
    XLA typically achieves: 50-60% MFU for training (vs 35-45% typical GPU)
    Higher MFU because:
      - Large SRAM reduces HBM bottleneck
      - XLA's fusion is tuned for TPU's systolic array
      - GSPMD minimizes communication overhead

  Memory savings from XLA optimization:
    Buffer reuse:          20-40% less peak memory
    Fusion (less intermediates): 10-30% less memory
    Layout optimization:   eliminates unnecessary transposes/copies

  Recompilation overhead (the pain point):
    New batch size:  ~5-30 seconds recompilation
    New seq length:  ~5-30 seconds recompilation
    This is why XLA-based serving uses bucketing.
```
