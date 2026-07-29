# ML Compiler Landscape — From Python to GPU Machine Code

---

## 1. The Problem ML Compilers Solve

```
You write:
  y = relu(x @ W + b)

PyTorch (eager mode) executes this as 3 separate GPU kernel launches:
  1. matmul_kernel(x, W) → temp1           (launch kernel, return to CPU)
  2. add_kernel(temp1, b) → temp2          (launch kernel, return to CPU)
  3. relu_kernel(temp2) → y               (launch kernel, return to CPU)

Each kernel launch:
  - CPU → GPU command queue (PCIe latency: ~5-10μs)
  - GPU reads input from HBM (~2TB/s bandwidth)
  - GPU computes (very fast)
  - GPU writes output to HBM
  - CPU gets notified, launches next kernel

The problem: HBM bandwidth is the bottleneck, not compute.
  matmul writes temp1 to HBM → add reads temp1 back → writes temp2 → relu reads temp2

  If you FUSED all three into ONE kernel:
    matmul computes result → stays in registers/SRAM → add → relu → write ONCE to HBM
    3× less HBM traffic. 1 kernel launch instead of 3.

ML compilers do this fusion automatically.
They turn your Python/framework code into optimized GPU kernels.
```

---

## 2. The Landscape — Who's Who

```
┌──────────────────────────────────────────────────────────────────────┐
│                     ML COMPILER LANDSCAPE (2024-2026)                │
│                                                                      │
│  FRAMEWORK LEVEL (what you write)                                   │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐        │
│  │   PyTorch      │  │     JAX        │  │  TensorFlow    │        │
│  │  (torch.compile)│  │  (jit/trace)   │  │  (tf.function) │        │
│  └───────┬────────┘  └───────┬────────┘  └───────┬────────┘        │
│          │                   │                   │                   │
│  GRAPH CAPTURE                                                      │
│  ┌───────▼────────┐  ┌──────▼─────────┐  ┌─────▼──────────┐       │
│  │  TorchDynamo   │  │    JAX trace   │  │   tf.function  │       │
│  │  (Python       │  │   (functional  │  │  (graph mode)  │       │
│  │   bytecode     │  │    transform)  │  │                │       │
│  │   interception)│  │               │  │                │       │
│  └───────┬────────┘  └──────┬─────────┘  └─────┬──────────┘       │
│          │                   │                   │                   │
│  HIGH-LEVEL IR (graph of tensor operations)                         │
│  ┌───────▼────────┐  ┌──────▼─────────┐  ┌─────▼──────────┐       │
│  │   FX Graph     │  │   StableHLO /  │  │  tf.Graph /    │       │
│  │   (PyTorch IR) │  │   HLO          │  │  MLIR          │       │
│  └───────┬────────┘  └──────┬─────────┘  └─────┬──────────┘       │
│          │                   │                   │                   │
│  OPTIMIZATION + LOWERING                                            │
│  ┌───────▼────────┐  ┌──────▼─────────┐  ┌─────▼──────────┐       │
│  │  Inductor      │  │     XLA        │  │   XLA / MLIR   │       │
│  │  (PyTorch      │  │  (Google's     │  │                │       │
│  │   compiler)    │  │   compiler)    │  │                │       │
│  └───────┬────────┘  └──────┬─────────┘  └─────┬──────────┘       │
│          │                   │                   │                   │
│  CODE GENERATION (produce actual GPU code)                          │
│  ┌───────▼────────┐  ┌──────▼─────────┐  ┌─────▼──────────┐       │
│  │   Triton       │  │  XLA codegen   │  │  XLA codegen   │       │
│  │  (Python→GPU   │  │  (HLO→LLVM→   │  │                │       │
│  │   kernels)     │  │   PTX/AMDGPU)  │  │                │       │
│  └───────┬────────┘  └──────┬─────────┘  └─────┬──────────┘       │
│          │                   │                   │                   │
│  LOW-LEVEL (hardware)                                               │
│  ┌───────▼───────────────────▼───────────────────▼──────────┐      │
│  │              LLVM → PTX → SASS (NVIDIA)                  │      │
│  │              LLVM → AMDGPU (AMD)                         │      │
│  │              Custom backends (TPU, Intel, etc.)           │      │
│  └──────────────────────────────────────────────────────────┘      │
│                                                                      │
│  CROSS-CUTTING INFRASTRUCTURE                                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                │
│  │    MLIR     │  │     TVM     │  │   IREE      │                │
│  │ (compiler   │  │ (end-to-end │  │ (MLIR-based │                │
│  │  framework) │  │  ML compiler)│  │  runtime)   │                │
│  └─────────────┘  └─────────────┘  └─────────────┘                │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. Each Project Explained

### 3.1 XLA (Accelerated Linear Algebra) — Google

```
WHAT: Whole-graph compiler. Takes a computation graph, optimizes it,
      generates code for GPU/TPU/CPU.

WHO:  Google. THE compiler behind JAX and TensorFlow on TPUs.

HOW IT WORKS:
  1. Input: HLO (High-Level Operations) IR
     A graph of tensor ops: matmul, conv, reduce, elementwise, etc.

  2. Optimization passes:
     - OPERATOR FUSION: matmul + bias + relu → one fused kernel
     - LAYOUT OPTIMIZATION: choose memory layout (row-major, tiled)
       that minimizes data movement for each op
     - ALGEBRAIC SIMPLIFICATION: x * 1 = x, redundant reshapes removed
     - BUFFER ASSIGNMENT: decide which ops can share memory buffers
     - SCHEDULING: order ops to maximize parallelism, minimize memory

  3. Code generation:
     For GPU: HLO → LLVM IR → PTX → SASS (NVIDIA machine code)
     For TPU: HLO → TPU-specific instructions (custom hardware)
     For CPU: HLO → LLVM IR → x86/ARM machine code

WHY IT EXISTS:
  Google built TPUs (custom AI chips). XLA is how they compile
  models to run on TPUs. Also works on GPUs, but TPU is the
  primary target.

STRENGTHS:
  - WHOLE-GRAPH optimization (sees everything, can do global fusion)
  - TPU support (only compiler that targets TPUs well)
  - Mature, production-tested (powers all of Google's AI)
  - Handles very large graphs (entire model in one compilation)

WEAKNESSES:
  - Requires STATIC shapes (all tensor dimensions known at compile time)
    Dynamic shapes need re-compilation → overhead
  - Long compilation times for large models (~minutes)
  - Not great for eager/interactive development
  - GPU kernels sometimes slower than hand-tuned CUDA (improving)

USED BY:
  - JAX (primary backend)
  - TensorFlow (optional, default for TPU)
  - PyTorch via torch-xla (for running PyTorch on TPUs)

StableHLO:
  Portable version of HLO. An MLIR dialect that other frameworks
  can target to use XLA's backend. Makes XLA accessible beyond
  just JAX/TF.
```

### 3.2 Triton — OpenAI → Now Part of PyTorch

```
WHAT: A language + compiler for writing GPU kernels in Python.
      Not a graph compiler — you write INDIVIDUAL kernels.

WHO:  Philippe Tillet (created at Harvard), then OpenAI, now
      maintained as part of the PyTorch ecosystem.

HOW IT WORKS:
  You write a kernel function in Python-like syntax:

    @triton.jit
    def fused_matmul_relu(X, W, Y, M, N, K, ...):
        pid = tl.program_id(0)              # which block am I?
        # Load a tile of X and W
        x = tl.load(X + offsets_x)          # load from HBM
        w = tl.load(W + offsets_w)
        acc = tl.dot(x, w)                  # matmul in SRAM
        acc = tl.maximum(acc, 0)            # relu, still in SRAM
        tl.store(Y + offsets_y, acc)         # write ONCE to HBM

  Triton compiles this:
    Python → Triton IR → LLVM IR → PTX → SASS

  KEY IDEA: you think in TILES (blocks of the matrix), not threads.
    CUDA: you manage threads, warps, shared memory, synchronization.
    Triton: you manage TILES. Compiler handles threads/warps/shmem.
    Much simpler to write. ~10× less code than equivalent CUDA.

WHY IT EXISTS:
  CUDA is hard. Writing a fast matmul kernel is ~500 lines of CUDA
  with manual shared memory management, bank conflict avoidance,
  warp shuffles, etc. Triton does the same in ~30 lines.

STRENGTHS:
  - Write GPU kernels in Python (huge accessibility win)
  - Auto-tuning: tries different tile sizes, picks the fastest
  - Good performance: 80-95% of hand-tuned CUDA for many kernels
  - Flash Attention is implemented in Triton

WEAKNESSES:
  - Still need to think about GPU architecture (tiles, memory hierarchy)
  - Not as fast as hand-tuned CUDA for the absolute critical kernels
    (cuBLAS matmul is still faster than Triton matmul for large sizes)
  - NVIDIA GPU only (AMD support via triton-amd, still maturing)
  - Individual kernels, not whole-graph optimization

USED BY:
  - PyTorch Inductor (generates Triton kernels as its GPU backend)
  - Flash Attention (original and many variants)
  - Many custom kernels in ML research
  - Increasingly used for production inference (vLLM, SGLang)
```

### 3.3 TorchDynamo + Inductor — PyTorch's Compiler

```
WHAT: PyTorch 2.0's compilation pipeline.
      TorchDynamo captures the graph. Inductor optimizes + generates code.

WHO:  Meta (PyTorch team).

HOW IT WORKS:

  @torch.compile
  def f(x):
      return relu(x @ W + b)

  Step 1: TORCHDYNAMO (graph capture)
    Intercepts Python BYTECODE at runtime.
    Watches what tensor ops your code does.
    Captures them into an FX Graph (PyTorch's IR).
    Handles Python control flow by "graph breaking":
      If it hits a Python if/print/etc, it compiles what
      it has so far, runs the Python part eagerly, then
      starts capturing again.

  Step 2: AOTAUTOGRAD (forward + backward graph)
    Traces the forward graph.
    Uses autograd rules to produce the backward graph.
    Now you have a joint forward+backward graph to optimize.

  Step 3: INDUCTOR (optimization + code generation)
    Takes the FX graph and:
    - Fuses elementwise ops (add + relu + dropout → one kernel)
    - Picks between Triton (custom fused kernels) and
      cuBLAS/cuDNN (for matmul/conv where vendor libs win)
    - Generates Triton kernel code for fused ops
    - Schedules memory allocation

  Step 4: TRITON (compile generated kernels)
    Triton kernels → LLVM → PTX → GPU machine code.
    Results are CACHED so second call is fast.

WHY IT EXISTS:
  PyTorch eager mode is easy to use but slow:
    - Python overhead per operation (~10-50μs)
    - No operator fusion (extra HBM traffic)
    - No graph-level optimization

  torch.compile: keep the eager-mode API but compile automatically.
  30-70% speedup on common models with one line of code.

STRENGTHS:
  - DROP-IN: just add @torch.compile. No code rewrite needed.
  - Handles dynamic shapes (unlike XLA, which needs static shapes)
  - Handles Python control flow via graph breaks
  - Uses Triton for fusion + cuBLAS for matmul (best of both)
  - Active development (Meta's primary focus)

WEAKNESSES:
  - Compilation time on first call (~30s for large models)
  - Graph breaks reduce optimization opportunities
  - Dynamic shapes still slower than static (more recompilation)
  - Newer, less mature than XLA
```

### 3.4 MLIR (Multi-Level Intermediate Representation) — LLVM/Google

```
WHAT: NOT a compiler itself. A FRAMEWORK for building compilers.
      Provides reusable infrastructure (IRs, passes, dialects).

WHO:  Chris Lattner (creator of LLVM, Swift) at Google, now
      an LLVM project.

WHAT IT IS:
  LLVM revolutionized general-purpose compilers:
    Every language (C, Rust, Swift) → LLVM IR → x86/ARM.
    Shared optimization passes. Don't reinvent the wheel.

  MLIR does the same for ML + domain-specific compilers:
    Multiple levels of IR, from high-level (tensor ops)
    to low-level (machine instructions).

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Dialects (modular, composable IR layers):                  │
  │                                                              │
  │  tensor dialect     ← high-level: tensor.matmul, tensor.relu │
  │       ↓                                                      │
  │  linalg dialect     ← loop structure: for i, for j, ...     │
  │       ↓                                                      │
  │  affine dialect     ← polyhedral: tiling, loop interchange  │
  │       ↓                                                      │
  │  scf dialect        ← structured control flow: for, if      │
  │       ↓                                                      │
  │  gpu dialect        ← GPU concepts: blocks, threads, shmem  │
  │       ↓                                                      │
  │  llvm dialect       ← LLVM IR: ready for LLVM codegen       │
  │       ↓                                                      │
  │  PTX / AMDGPU       ← actual GPU assembly                   │
  │                                                              │
  │  Each level has its own optimizations.                      │
  │  You can enter/exit at any level.                           │
  │  You can define your OWN dialects for new hardware.         │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

WHY IT MATTERS:
  Every hardware company (NVIDIA, AMD, Intel, Google, Apple,
  Qualcomm, startups making AI chips) needs a compiler for ML.
  Without MLIR: each builds their own IR, passes, tools from scratch.
  With MLIR: reuse the infrastructure, only write the new parts.

WHO USES IT:
  - XLA (being rewritten to use MLIR internally via StableHLO)
  - TensorFlow (MLIR is the compiler infrastructure since TF 2.x)
  - IREE (Google's MLIR-based runtime for edge/mobile)
  - Torch-MLIR (PyTorch → MLIR)
  - Many AI chip startups (Cerebras, SambaNova, Groq)
  - Apple (Core ML compiler uses MLIR)
  - AMD (ROCm compiler stack)
```

### 3.5 TVM (Apache TVM) — End-to-End ML Compiler

```
WHAT: Full-stack compiler: takes an ML model, optimizes it,
      generates code for MANY hardware targets.

WHO:  Created by Tianqi Chen (UW, also created XGBoost and MLC-LLM).
      Apache project. OctoML (commercial company).

HOW IT WORKS:
  1. Import model from PyTorch/TF/ONNX
  2. Relay IR (high-level graph)
     - Graph-level optimization: fusion, layout transform, constant folding
  3. TIR (Tensor IR, low-level)
     - Loop-level optimization: tiling, vectorization, unrolling
     - SCHEDULE: you can manually specify or AUTO-TUNE
  4. Code generation for target hardware

  ┌─────────────────────────────────────────────────────────────┐
  │                                                             │
  │  PyTorch model ──► Relay IR ──► TIR ──► Target code        │
  │                                                             │
  │  Targets:                                                   │
  │    NVIDIA GPU (CUDA)                                       │
  │    AMD GPU (ROCm)                                          │
  │    Intel CPU (x86 + AVX-512)                               │
  │    ARM CPU (mobile, Apple M-series)                        │
  │    ARM GPU (Mali)                                          │
  │    FPGA                                                    │
  │    Custom accelerators (via BYOC — Bring Your Own Codegen) │
  │    WebGPU (run in browser!)                                │
  │                                                             │
  └─────────────────────────────────────────────────────────────┘

KEY FEATURE: AUTO-TUNING (AutoTVM / Meta-Schedule)
  Instead of hand-writing optimization schedules:
  - Generate thousands of candidate implementations
  - Benchmark each on actual hardware
  - ML model predicts which candidates are promising
  - Pick the fastest
  This finds optimizations that humans miss.

STRENGTHS:
  - BROADEST hardware support (the most targets of any ML compiler)
  - Auto-tuning finds hardware-specific optimizations
  - Good for deployment/inference (edge, mobile, embedded)
  - Active community, well-documented

WEAKNESSES:
  - Auto-tuning is SLOW (hours to tune a model for a new target)
  - Training support is limited (primarily an inference compiler)
  - Less integrated with PyTorch than Inductor/Triton
  - Compilation pipeline is complex (steep learning curve)

USED BY:
  - OctoML (commercial TVM company)
  - Qualcomm (AI Engine uses TVM)
  - Amazon (SageMaker Neo uses TVM for deployment)
  - MLC-LLM (run LLMs on phones/browsers via TVM + WebGPU)
```

### 3.6 IREE (Intermediate Representation Execution Environment) — Google

```
WHAT: MLIR-based compiler + RUNTIME for deploying ML models.
      Compiles models into efficient, portable executables.

WHO:  Google (open-source).

HOW IT WORKS:
  Model (PyTorch/TF/JAX) → StableHLO → MLIR passes → IREE VM bytecode
  → Target-specific codegen (Vulkan, CUDA, CPU, SPIR-V)

  KEY IDEA: compile once, run anywhere.
  The IREE VM is a lightweight runtime that can execute on:
    - Desktop GPUs via Vulkan
    - NVIDIA GPUs via CUDA
    - CPUs via LLVM
    - Mobile GPUs via Vulkan/Metal
    - WebGPU (in browser)

WHY IT EXISTS:
  XLA is great for Google's TPUs but heavy for edge devices.
  IREE is designed for LOW-LATENCY, SMALL-FOOTPRINT deployment.
  Think: running a model on a phone or embedded device.

USED BY:
  - Google (internal edge deployment)
  - AMD (ROCm integration)
  - Various edge AI deployments
```

### 3.7 Torch-TensorRT — NVIDIA

```
WHAT: Integrates NVIDIA's TensorRT optimizer into PyTorch.

WHO:  NVIDIA.

HOW IT WORKS:
  PyTorch model → torch.compile with TensorRT backend
  → TensorRT optimizes: kernel fusion, FP16/INT8 quantization,
    layer and tensor fusion, kernel auto-tuning
  → Runs on NVIDIA GPUs with TensorRT runtime

  TensorRT is NVIDIA's INFERENCE optimizer (not training).
  It knows NVIDIA hardware intimately → often fastest on NVIDIA GPUs.

STRENGTHS:
  - Fastest inference on NVIDIA GPUs (period)
  - INT8/FP8 quantization support (smaller, faster)
  - Integrated into PyTorch via torch.compile backend

WEAKNESSES:
  - NVIDIA only (no AMD, no CPU)
  - Inference only (not training)
  - Static shapes preferred (dynamic shapes have overhead)
  - Closed-source core (TensorRT engine)
```

### 3.8 ONNX Runtime — Microsoft

```
WHAT: Cross-platform inference engine with compilation.
      ONNX = Open Neural Network Exchange (model format).

WHO:  Microsoft.

HOW IT WORKS:
  PyTorch model → export to ONNX format
  → ONNX Runtime applies graph optimizations
  → Execution on: CPU (Intel/ARM), NVIDIA GPU, AMD GPU,
    Intel GPU, Qualcomm NPU, DirectML, OpenVINO, TensorRT

  It's the MOST PORTABLE inference runtime.
  "Export once, run on any hardware with optimized backends."

STRENGTHS:
  - Broadest deployment support (Windows, Linux, iOS, Android, web)
  - Good performance across all platforms
  - Microsoft uses it for everything (Azure, Office, Windows)
  - Training support via ONNX Runtime Training (ORTModule)

WEAKNESSES:
  - ONNX export from PyTorch can be lossy (not all ops supported)
  - Not as fast as TensorRT on NVIDIA, or XLA on TPU
  - Middle ground: good everywhere, best nowhere
```

### 3.9 Other Notable Projects

```
cuDNN:
  NVIDIA's hand-tuned library of neural network primitives.
  Conv2d, BatchNorm, LSTM, attention — all hand-optimized in CUDA.
  Not a compiler, but the PERFORMANCE TARGET all compilers try to match.
  When Inductor generates a conv2d, it just calls cuDNN.

cuBLAS:
  NVIDIA's hand-tuned matrix multiply library.
  The fastest matmul on NVIDIA GPUs. Period.
  Triton/XLA generate their own matmuls but often fall short for large sizes.

Flash Attention:
  Not a compiler, but a CUSTOM KERNEL (written in Triton/CUDA).
  Fuses the entire attention computation into one kernel:
    Q @ K.T → softmax → @ V, all in SRAM.
  10× less HBM traffic than standard attention.
  So important that all compilers now have special support for it.

FlexAttention (PyTorch 2.5+):
  PyTorch's built-in attention compiler.
  You define a custom "score modification" function in Python:
    def causal_mask(score, b, h, q, k):
        return torch.where(q >= k, score, -float('inf'))
  FlexAttention compiles this into a fused attention kernel via Triton.
  Makes it easy to experiment with new attention patterns without CUDA.

Mojo:
  New language by Chris Lattner (LLVM/Swift creator).
  Python-like syntax that compiles to MLIR → GPU/CPU.
  Aims to replace both Python (for writing) and C++/CUDA (for performance).
  Still early. Ambitious goal.

JAX pjit / shard_map:
  JAX's approach to distributed compilation.
  Annotate which tensor axes are sharded across which devices.
  XLA compiler figures out the communication (AllReduce, etc.) automatically.
  This is GSPMD: automatic parallelism from sharding annotations.
```

---

## 4. How They Relate — The Big Picture

```
The key question: WHERE does the compiler sit?

  GRAPH LEVEL (whole model optimization):
    XLA:        graph → optimized graph → GPU/TPU code
    Inductor:   FX graph → optimized graph → Triton kernels + cuBLAS calls
    TVM:        Relay graph → TIR → target code
    TensorRT:   graph → fused + quantized → NVIDIA runtime
    ONNX RT:    ONNX graph → optimized graph → hardware backends

  KERNEL LEVEL (individual operation optimization):
    Triton:     single kernel function → GPU code
    cuDNN:      hand-tuned kernel library (pre-compiled)
    cuBLAS:     hand-tuned matmul library (pre-compiled)

  INFRASTRUCTURE (build your own compiler):
    MLIR:       framework of reusable compiler passes and IRs
    LLVM:       low-level code generation (shared by all)

  The graph-level compilers CALL kernel-level tools:
    Inductor generates Triton kernels for elementwise fusion
    but calls cuBLAS for matmul (because cuBLAS is still faster).

    XLA has its own codegen for most ops but uses cuDNN/cuBLAS
    for operations where vendor libraries are superior.
```

---

## 5. Comparison Table

```
┌───────────────┬──────────┬──────────────┬──────────────┬───────────────┐
│               │ XLA      │ Inductor     │ Triton       │ TVM           │
├───────────────┼──────────┼──────────────┼──────────────┼───────────────┤
│ Level         │ Graph    │ Graph        │ Kernel       │ Graph+Kernel  │
│ Framework     │ JAX, TF  │ PyTorch      │ Any          │ Any (ONNX)    │
│ Training      │ Yes      │ Yes          │ Yes (manual) │ Limited       │
│ Inference     │ Yes      │ Yes          │ Yes          │ Yes (primary) │
│ GPU (NVIDIA)  │ Yes      │ Yes          │ Yes          │ Yes           │
│ GPU (AMD)     │ Partial  │ Partial      │ Partial      │ Yes           │
│ TPU           │ Yes(only)│ No           │ No           │ No            │
│ CPU           │ Yes      │ Yes          │ No           │ Yes           │
│ Mobile/Edge   │ No       │ No           │ No           │ Yes           │
│ Dynamic shapes│ Limited  │ Yes          │ Yes          │ Limited       │
│ Ease of use   │ Medium   │ Easy(@compile│ Medium       │ Hard          │
│               │          │  1-line)     │ (write kernel│ (need tuning) │
│ Maturity      │ Very high│ Medium       │ High         │ High          │
│ Open source   │ Yes      │ Yes          │ Yes          │ Yes (Apache)  │
├───────────────┼──────────┼──────────────┼──────────────┼───────────────┤
│ Best for      │ JAX/TPU  │ PyTorch users│ Custom ops,  │ Deploy to     │
│               │ workloads│ wanting speed│ fused kernels│ many targets  │
└───────────────┴──────────┴──────────────┴──────────────┴───────────────┘

┌───────────────┬──────────────┬──────────────┬──────────────────────────┐
│               │ TensorRT     │ ONNX Runtime │ MLIR                     │
├───────────────┼──────────────┼──────────────┼──────────────────────────┤
│ Level         │ Graph        │ Graph        │ Infrastructure           │
│ Focus         │ Inference    │ Inference    │ Building compilers       │
│ GPU (NVIDIA)  │ Yes (best)   │ Yes          │ via dialects             │
│ Portability   │ NVIDIA only  │ Everywhere   │ Any target               │
│ Quantization  │ INT8/FP8     │ INT8         │ Custom                   │
│ Open source   │ Partial      │ Yes          │ Yes (LLVM project)       │
├───────────────┼──────────────┼──────────────┼──────────────────────────┤
│ Best for      │ NVIDIA       │ Portable     │ Chip companies building  │
│               │ inference    │ deployment   │ their own ML compilers   │
└───────────────┴──────────────┴──────────────┴──────────────────────────┘
```

---

## 6. The Compilation Pipeline for Each Stack

```
PYTORCH (2024+):
  Python code
    → TorchDynamo (captures FX graph via bytecode interception)
    → AOTAutograd (produces forward + backward graphs)
    → Inductor (graph optimization + scheduling)
    → Triton (for fused elementwise) + cuBLAS (for matmul) + cuDNN (for conv)
    → LLVM → PTX → SASS (NVIDIA GPU machine code)

JAX:
  Python code
    → JAX tracing (captures Jaxpr)
    → StableHLO (portable tensor IR)
    → XLA (graph optimization + codegen)
    → LLVM → PTX (GPU) or TPU instructions (TPU)

TENSORFLOW:
  Python code
    → tf.function (captures graph)
    → MLIR-based optimization passes
    → XLA (optional, required for TPU)
    → LLVM → PTX (GPU) or TPU instructions

TVM:
  ONNX / PyTorch model
    → Relay IR (graph level)
    → TIR (tensor IR, loop level)
    → Auto-tuning (search for best schedule)
    → LLVM → PTX / ARM / x86 / WebGPU / whatever
```

---

## 7. Why This Matters for Training vs Inference

```
TRAINING:
  - torch.compile + Inductor: 30-70% speedup over eager, easy to use
  - XLA + JAX: best for TPUs, good for research-scale GPU training
  - Custom Triton kernels: when you need a specialized fused op
    (Flash Attention, fused Adam, fused layernorm+dropout)

  Training compilers need to handle:
    - Forward AND backward pass
    - Dynamic batch sizes, sequence lengths
    - Gradient accumulation, mixed precision
    - Distributed: must cooperate with NCCL

INFERENCE (where compilers matter even MORE):
  - TensorRT: fastest on NVIDIA (INT8/FP8 quantization)
  - ONNX Runtime: most portable
  - TVM: best for edge/mobile
  - vLLM/SGLang use Triton kernels for PagedAttention

  Inference compilers can:
    - Quantize: FP32 → INT8 (4× smaller, 2-4× faster)
    - Batch: fuse multiple requests
    - Static shapes: compile once, run many times (100% optimized)
    - Remove training-only ops (dropout, grad computation)

The industry is converging:
  2020: separate worlds (XLA for Google, TorchScript for PyTorch, TVM for deploy)
  2024: MLIR as shared infrastructure, Triton as shared kernel language
  2026: increasingly unified — PyTorch ecosystem (Dynamo+Inductor+Triton)
        dominates training, with TensorRT/ONNX for inference deployment
```

---

## 8. Key Optimization Techniques All Compilers Use

```
1. OPERATOR FUSION
   matmul → add → relu: 3 kernels, 3 HBM reads/writes
   fused: 1 kernel, 1 HBM read, 1 write. 3× less memory traffic.
   This is the #1 optimization. All compilers do it.

2. MEMORY PLANNING
   Tensors A and B are both temporary but never alive at the same time.
   → Reuse the same memory buffer for both. Reduces peak memory.

3. LAYOUT OPTIMIZATION
   Row-major vs column-major vs tiled layout.
   Different ops prefer different layouts. Compiler inserts conversions
   only where necessary and picks the best layout per subgraph.

4. CONSTANT FOLDING
   Weights are constants during inference.
   Precompute anything that depends ONLY on weights at compile time.

5. KERNEL SELECTION
   For matmul: use cuBLAS (hand-tuned, fastest).
   For elementwise chain: generate a Triton kernel (compilers are better).
   For attention: use Flash Attention (hand-tuned special case).
   Good compilers KNOW when to call vendor libs vs generate their own code.

6. TILING
   Large matrix ops: split into tiles that fit in SRAM (shared memory).
   Process one tile at a time. Avoid going to HBM for intermediate values.
   This is what makes Flash Attention fast.

7. AUTO-TUNING
   Generate many candidate implementations with different tile sizes,
   loop orders, vectorization choices. Run each on actual hardware.
   Keep the fastest. TVM and Triton both do this.
```
