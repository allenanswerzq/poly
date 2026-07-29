# TensorRT — NVIDIA's Inference Compiler & Optimizer

---

## 1. What TensorRT Is

```
TensorRT is NVIDIA's AHEAD-OF-TIME compiler for neural network inference.
It takes a trained model and produces a maximally optimized GPU executable.

  The distinction:
    TensorRT        = the compiler/optimizer engine (general, any NN)
    TensorRT-LLM    = LLM-specific inference framework BUILT ON TensorRT
    Torch-TensorRT  = PyTorch integration layer that calls TensorRT

  TensorRT is the foundation underneath both.

  What it does:
    Input:  trained model (ONNX, TF, or framework-specific)
    Output: .engine file (serialized optimized GPU program)

    The engine is SPECIFIC to:
      - GPU architecture (A100, H100, B200 — different SASS)
      - Max batch size, input shapes
      - Precision config (FP32, FP16, FP8, INT8)
      - Plugin configuration

    You CANNOT take an engine built for A100 and run it on H100.
    Must rebuild for each target GPU.

  Why it's fast:
    TensorRT knows NVIDIA hardware INTIMATELY — it's made by NVIDIA.
    It selects from thousands of pre-tuned kernel implementations,
    benchmarks them on YOUR GPU, and picks the fastest combination.
    No other compiler has this level of NVIDIA-specific tuning.

  GitHub: https://github.com/NVIDIA/TensorRT (partially open-source)
  Core engine: closed-source (libnvinfer.so)
  Plugins, parsers, samples: open-source
```

---

## 2. How TensorRT Compiles a Model

```
The compilation pipeline:

  ┌─────────────────────────────────────────────────────────────┐
  │ Step 1: IMPORT — Parse the model graph                     │
  │                                                             │
  │   Sources:                                                  │
  │     ONNX model    → ONNX parser (most common path)         │
  │     PyTorch       → torch.export → ONNX → TensorRT         │
  │     TF SavedModel → TF-TRT / ONNX conversion               │
  │     TensorRT API  → build graph directly in C++/Python      │
  │                                                             │
  │   Result: TensorRT Network Definition (internal graph)      │
  └──────────────────────────┬──────────────────────────────────┘
                             │
                             ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Step 2: OPTIMIZE — Graph-level transformations              │
  │                                                             │
  │   Layer fusion:                                             │
  │     Conv + BatchNorm + ReLU → single CBR kernel             │
  │     Linear + Bias + GELU → single fused kernel              │
  │     QKV projections → one batched GEMM                      │
  │     Elementwise chains → single pointwise kernel            │
  │                                                             │
  │   Precision calibration:                                    │
  │     FP32 → FP16: automatic (just truncate)                  │
  │     FP32 → INT8: requires calibration dataset               │
  │       Run ~500 samples, measure activation ranges           │
  │       Compute per-tensor or per-channel scale factors       │
  │     FP32 → FP8: similar to INT8 (H100+ only)              │
  │                                                             │
  │   Constant folding:                                         │
  │     Precompute anything that depends only on weights        │
  │     Fold BatchNorm into Conv weights                        │
  │                                                             │
  │   Dead layer elimination:                                   │
  │     Remove outputs nobody reads                             │
  │     Remove identity operations                              │
  └──────────────────────────┬──────────────────────────────────┘
                             │
                             ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Step 3: KERNEL AUTO-TUNING — The secret sauce               │
  │                                                             │
  │   For EACH fused operation, TensorRT has a library of       │
  │   kernel implementations (called "tactics").                │
  │                                                             │
  │   Example: a [1024 × 4096] × [4096 × 4096] GEMM           │
  │     Tactic A: cuBLAS GEMM, tile 128×128, split-K           │
  │     Tactic B: cuBLAS GEMM, tile 256×64                     │
  │     Tactic C: cuBLAS GEMM with HMMA (tensor core)          │
  │     Tactic D: custom Myelin-generated kernel                │
  │     Tactic E: cuBLASLt with epilogue fusion                 │
  │     ... 20-50+ tactics per layer                            │
  │                                                             │
  │   TensorRT BENCHMARKS EVERY TACTIC on your actual GPU.     │
  │   Picks the fastest. This is why build takes minutes-hours. │
  │                                                             │
  │   Different GPU same arch → different winning tactics       │
  │   (thermals, clock speed, memory bandwidth vary).           │
  └──────────────────────────┬──────────────────────────────────┘
                             │
                             ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Step 4: MEMORY PLANNING                                     │
  │                                                             │
  │   Static allocation: compute the EXACT memory layout.       │
  │   No malloc/free during inference. Zero allocation overhead.│
  │                                                             │
  │   Tensors with non-overlapping lifetimes share buffers.    │
  │   Scratch space pre-allocated for all kernels.             │
  │   Result: known peak memory before any inference runs.      │
  └──────────────────────────┬──────────────────────────────────┘
                             │
                             ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ Step 5: SERIALIZE — Write the .engine file                  │
  │                                                             │
  │   Contains:                                                 │
  │     - Selected kernel code (PTX/SASS)                       │
  │     - Memory layout plan                                    │
  │     - Weights (possibly quantized)                          │
  │     - Execution schedule (kernel launch order + streams)    │
  │                                                             │
  │   Size: often similar to model weights.                    │
  │   Load time: fast (deserialize + allocate GPU memory).      │
  └─────────────────────────────────────────────────────────────┘
```

---

## 3. Kernel Tactics & Myelin — Why TensorRT Is Fast

```
TensorRT's performance advantage comes from two things:

  1. TACTIC LIBRARY:
     NVIDIA maintains thousands of hand-tuned kernel implementations.
     For a single GEMM shape, there might be 50 tactics:
       - Different tile sizes (64×64, 128×128, 256×64)
       - Different data paths (HMMA for tensor cores, FFMA for CUDA cores)
       - Different memory access patterns (split-K, sliced-K)
       - Different epilogue fusions (bias, activation, residual)

     cuBLAS alone has hundreds of GEMM kernels.
     TensorRT also includes cuDNN tactics for conv/attention,
     plus its own generated kernels.

     No other compiler has access to this NVIDIA-internal library.

  2. MYELIN (internal code generator):
     For fused operations that don't match existing tactics,
     TensorRT uses Myelin to GENERATE custom kernels:
       - Pointwise fusion (chains of elementwise ops)
       - Small operations that don't warrant cuBLAS
       - Custom epilogues after GEMM

     Myelin is a small compiler within TensorRT.
     Not publicly documented, but it generates PTX/SASS directly.

  ┌──────────────────────────────────────────────────────────┐
  │ Example: Transformer layer compilation                   │
  │                                                          │
  │ Before TensorRT:                                         │
  │   LayerNorm         → 1 kernel                           │
  │   Q projection      → 1 cuBLAS GEMM                     │
  │   K projection      → 1 cuBLAS GEMM                     │
  │   V projection      → 1 cuBLAS GEMM                     │
  │   QK^T              → 1 kernel                           │
  │   Scale + mask      → 1 kernel                           │
  │   Softmax           → 1 kernel                           │
  │   Attn @ V          → 1 kernel                           │
  │   Output projection → 1 cuBLAS GEMM                     │
  │   Residual add      → 1 kernel                           │
  │   LayerNorm         → 1 kernel                           │
  │   MLP linear 1      → 1 cuBLAS GEMM                     │
  │   GELU              → 1 kernel                           │
  │   MLP linear 2      → 1 cuBLAS GEMM                     │
  │   Residual add      → 1 kernel                           │
  │   = 15 kernel launches                                   │
  │                                                          │
  │ After TensorRT:                                          │
  │   LayerNorm + QKV fused GEMM  → 1 kernel                │
  │   Flash Attention (fused)     → 1 kernel                 │
  │   Output proj + residual      → 1 kernel (GEMM+add)     │
  │   LayerNorm + MLP1 + GELU    → 1-2 kernels              │
  │   MLP2 + residual            → 1 kernel (GEMM+add)      │
  │   = 5-6 kernel launches                                  │
  │                                                          │
  │ ~3× fewer kernels → less HBM traffic, less launch overhead│
  └──────────────────────────────────────────────────────────┘
```

---

## 4. Quantization in TensorRT

```
TensorRT has the deepest quantization support on NVIDIA GPUs.

  ┌──────────────────────────────────────────────────────────┐
  │ Precision │ Hardware        │ How it works               │
  ├───────────┼─────────────────┼────────────────────────────┤
  │ FP32      │ Any NVIDIA GPU  │ Baseline, no quantization  │
  │ FP16      │ Any since Pascal│ Automatic downcast, safe   │
  │ BF16      │ Ampere+         │ Like FP16, wider range     │
  │ INT8      │ Turing+ (T4+)   │ Needs calibration data     │
  │ FP8       │ Hopper+ (H100+) │ Needs calibration data     │
  │ INT4      │ Any (weight-only)│ Weights only, compute FP16│
  │ FP4       │ Blackwell (B200)│ Native FP4 tensor cores    │
  └───────────┴─────────────────┴────────────────────────────┘

  INT8 QUANTIZATION (the most complex):

    Post-Training Quantization (PTQ):
      1. Run calibration: feed ~500 representative samples
         through the FP32 model.
      2. For each tensor, record the distribution of values.
      3. Compute optimal scale factor:
           scale = max(abs(tensor)) / 127  (symmetric)
           or use entropy calibration (minimize KL divergence
           between FP32 and INT8 distributions).
      4. At inference:
           INT8_tensor = round(FP32_tensor / scale)
           GEMM in INT8 (DP4A instruction: 4× throughput)
           Accumulate in INT32
           Dequantize output back to FP16/FP32

    Per-tensor vs per-channel:
      Per-tensor: one scale for entire weight matrix. Faster.
      Per-channel: one scale per output channel. More accurate.
      TensorRT supports both. Per-channel is default for weights.

  FP8 QUANTIZATION (H100+):

    Simpler than INT8:
      FP8 E4M3 format: 4 exponent bits, 3 mantissa bits.
      Wide dynamic range (exponent helps) → less sensitivity
      to outliers than INT8.
      Calibration still needed but less critical.

    H100 FP8 Tensor Cores: 1979 TFLOPS (vs 989 FP16).
    2× compute throughput + 2× less memory bandwidth.
    Quality nearly identical to FP16 for most models.

    This is why FP8 on H100 is the new default for inference.
```

---

## 5. Dynamic Shapes & Optimization Profiles

```
TensorRT historically required STATIC shapes (all dimensions
known at build time). This is a problem for:
  - Variable batch size (1 at low traffic, 64 at peak)
  - Variable sequence length (different prompt lengths)

Solution: OPTIMIZATION PROFILES

  builder_config.add_optimization_profile(profile)
  profile.set_shape("input",
      min  = (1,   1,    hidden),    # minimum: batch=1, seq=1
      opt  = (32,  512,  hidden),    # optimal: batch=32, seq=512
      max  = (128, 4096, hidden))    # maximum: batch=128, seq=4096

  TensorRT builds an engine that works for ANY shape within
  the min-max range, but is TUNED for the opt shape.

  At runtime: when you pass input with shape (16, 256, hidden),
  TensorRT selects the best tactic for that specific shape from
  its pre-benchmarked options.

  Multiple profiles:
    Profile 1: optimized for batch=1, seq=2048 (latency mode)
    Profile 2: optimized for batch=64, seq=512 (throughput mode)
    One engine, multiple sweet spots.

  Tradeoff:
    Wider min-max range → more generic (slower at any one shape)
    Narrow min-max range → more specialized (faster at that shape)
    This is why TRT-LLM builds separate engines for different configs.
```

---

## 6. Plugins — Extending TensorRT

```
Not every operation is natively supported by TensorRT.
PLUGINS let you add custom kernel implementations.

  Example: Flash Attention is a TensorRT plugin.
  TensorRT doesn't natively fuse attention into one kernel.
  Instead, a plugin provides the Flash Attention implementation.

  Plugin interface:
    class FlashAttnPlugin : public IPluginV2 {
      // describe input/output shapes, data types
      // enqueue(): run CUDA kernel
      // serialize/deserialize for engine persistence
    };

  Register the plugin, and TensorRT treats it like a native layer.
  It can still fuse surrounding ops with the plugin's inputs/outputs.

  Key plugins in TensorRT-LLM:
    - Flash Attention / MHA / GQA attention variants
    - RoPE (rotary position embedding)
    - RMS LayerNorm (used by Llama-style models)
    - Paged KV cache attention
    - MoE (mixture of experts) routing + computation

  The plugin system is why TensorRT can be extended to new
  architectures without modifying the core engine.
```

---

## 7. CUDA Graphs — Eliminating Launch Overhead

```
Problem: even after fusion, each kernel launch has CPU overhead.
  CPU enqueues kernel → GPU executes → CPU enqueues next → ...
  For small batches (decode phase), launch overhead dominates.

CUDA Graph: record a sequence of kernel launches ONCE,
then replay the entire sequence with a single CPU call.

  Without CUDA Graph:
    CPU: launch K1 → wait → launch K2 → wait → launch K3 → ...
    GPU: ........[K1].........[K2]........[K3]
         ↑ gaps = CPU overhead between kernel launches

  With CUDA Graph:
    CPU: replay graph (one call)
    GPU: [K1][K2][K3][K4][K5]...
         ↑ no gaps — all kernels queued at once

  TensorRT builds CUDA Graphs for the entire model execution.
  Especially impactful for LLM decode (each step is a small
  model forward pass — lots of kernels, little compute per kernel).

  Speedup: 10-30% for decode-heavy workloads.
```

---

## 8. TensorRT vs Other Compilers

```
  ┌──────────────────────────────────────────────────────────────┐
  │              │ TensorRT      │ torch.compile  │ XLA          │
  ├──────────────┼───────────────┼────────────────┼──────────────┤
  │ Type         │ Ahead-of-time │ JIT            │ Ahead-of-time│
  │ Target       │ NVIDIA only   │ NVIDIA + AMD   │ GPU + TPU    │
  │ Training     │ No            │ Yes            │ Yes          │
  │ Inference    │ Yes (primary) │ Yes            │ Yes          │
  │ Quantization │ Best on NVIDIA│ Basic          │ Limited      │
  │ Dynamic shape│ Via profiles  │ Native         │ Recompile    │
  │ Kernel tuning│ Tactic search │ Triton autotune│ XLA autotuner│
  │              │ (NVIDIA libs) │                │              │
  │ Build time   │ Minutes-hours │ Seconds        │ Minutes      │
  │ Open source  │ Partial       │ Yes            │ Yes          │
  │ Vendor lock  │ NVIDIA        │ No             │ No           │
  ├──────────────┼───────────────┼────────────────┼──────────────┤
  │ Best for     │ Max inference │ Training +     │ JAX/TPU      │
  │              │ perf on NVIDIA│ flexible infer │ workloads    │
  └──────────────┴───────────────┴────────────────┴──────────────┘

  Why TensorRT wins on NVIDIA inference:
    1. Access to NVIDIA's internal tactic library (no one else has this)
    2. Knows GPU microarchitecture details (register file size,
       L2 cache partitioning, tensor core scheduling)
    3. Can use closed-source NVIDIA-only instructions
    4. Memory planning is perfect (static, no runtime allocation)

  Why you might NOT use TensorRT:
    1. NVIDIA-only (no AMD, no TPU, no CPU)
    2. Build time (10 min for small model, hours for large)
    3. Engine not portable (must rebuild per GPU model)
    4. No training support
    5. Partial open-source (hard to debug core issues)
```

---

## 9. How TensorRT Relates to TensorRT-LLM

```
  TensorRT:
    General-purpose NN inference compiler.
    Supports CNNs, transformers, detection models, any NN.
    No LLM-specific features (no KV cache, no batching, no sampling).

  TensorRT-LLM:
    LLM-specific inference engine BUILT ON TOP of TensorRT.
    Adds everything needed for LLM serving:
      - KV cache management (paged)
      - In-flight batching (continuous batching)
      - Speculative decoding
      - Multi-GPU (TP + PP via NCCL)
      - Sampling (top-k, top-p, beam search)
      - Streaming token output

  ┌──────────────────────────────────────────────────────┐
  │                                                      │
  │  TensorRT-LLM                                       │
  │  ┌──────────────────────────────────────────────┐   │
  │  │ Scheduler, KV cache, batching, sampling      │   │
  │  │ Multi-GPU, speculative decoding              │   │
  │  └────────────────────┬─────────────────────────┘   │
  │                       │ uses                        │
  │  ┌────────────────────▼─────────────────────────┐   │
  │  │ TensorRT                                     │   │
  │  │ Kernel selection, fusion, quantization,      │   │
  │  │ memory planning, CUDA graph execution        │   │
  │  └──────────────────────────────────────────────┘   │
  │                                                      │
  └──────────────────────────────────────────────────────┘

  Similarly:
    Torch-TensorRT = PyTorch integration → calls TensorRT
    NVIDIA Triton Inference Server = serving infra → can use TRT engines
```

---

## 10. Key Numbers

```
TensorRT performance characteristics:

  Build time (engine compilation):
    ResNet-50:           ~30 seconds
    BERT-large:          ~2-5 minutes
    Llama-3 8B:          ~5-10 minutes
    Llama-3 70B (TP=8):  ~30-60 minutes

  Inference speedup over PyTorch eager:
    CNN models:          2-5× faster
    BERT/encoder models: 2-4× faster
    LLM (via TRT-LLM):  1.5-3× faster (mostly from quantization)

  Quantization impact (H100):
    FP16 → FP8:   ~1.5-2× throughput (compute-bound ops)
    FP16 → INT8:  ~1.5-2× throughput (similar to FP8)
    FP16 → INT4:  ~1.3-1.5× decode (memory-bound, weight-only)

  CUDA Graph overhead reduction:
    Without: ~5-10μs per kernel launch overhead
    With:    ~1 CPU call for entire model forward pass
    Impact:  10-30% speedup for small-batch / decode workloads

  Engine file sizes:
    ResNet-50 FP16:     ~50 MB
    Llama-3 8B FP8:     ~8 GB
    Llama-3 70B FP8:    ~70 GB (split across TP shards)
```
