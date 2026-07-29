# MLC-LLM — Run LLMs Everywhere via Compilation

---

## 1. What MLC-LLM Does

```
MLC-LLM (Machine Learning Compilation for LLMs) solves:
how do you run LLMs on phones, laptops, browsers, and
other consumer devices WITHOUT a big GPU server?

  The problem:
    Llama-3 8B in FP16 = 16 GB. An iPhone has 6-8 GB RAM.
    Even if the model fits (after quantization), you need
    efficient kernels for the specific hardware: Apple GPU,
    Qualcomm Adreno, WebGPU in a browser, etc.
    cuBLAS and cuDNN don't exist on these platforms.

  MLC-LLM: compile the model into optimized native code
  for ANY target hardware, using Apache TVM as the compiler.

  ┌──────────────────────────────────────────────────────────┐
  │  Supported targets:                                      │
  │                                                          │
  │  • iPhone / iPad         (Metal GPU)                     │
  │  • Android phones        (OpenCL / Vulkan GPU)           │
  │  • Mac / Windows / Linux (Metal / Vulkan / CUDA)         │
  │  • Web browsers          (WebGPU — runs in Chrome/Edge)  │
  │  • Embedded / edge       (CPU: ARM, x86)                 │
  │                                                          │
  │  Same model, same code path, compiled differently        │
  │  for each platform. No CUDA required.                    │
  └──────────────────────────────────────────────────────────┘

  Created by: Tianqi Chen (UW, also created TVM, XGBoost)
  GitHub: https://github.com/mlc-ai/mlc-llm

  Key idea: don't hand-write kernels for every platform.
  Use a COMPILER to generate them automatically.
```

---

## 2. How It Works — The Compilation Pipeline

```
MLC-LLM uses Apache TVM under the hood:

  ┌─────────────────────────────────────────────────────────┐
  │ Step 1: Import Model                                    │
  │                                                         │
  │   HuggingFace checkpoint (Llama, Mistral, Qwen, etc.)  │
  │   → Convert to TVM's Relax IR (high-level tensor graph) │
  │   → Apply quantization (INT4, INT3, FP4)               │
  └──────────────────────────┬──────────────────────────────┘
                             │
                             ▼
  ┌─────────────────────────────────────────────────────────┐
  │ Step 2: Compile                                         │
  │                                                         │
  │   TVM Relax IR                                          │
  │   → Graph-level optimization (fusion, layout transform) │
  │   → Loop-level optimization (tiling, vectorization)     │
  │   → Target-specific code generation:                    │
  │       Metal shaders (Apple GPU)                         │
  │       Vulkan SPIR-V  (Android / desktop GPU)            │
  │       WGSL           (WebGPU / browser)                 │
  │       CUDA           (NVIDIA GPU)                       │
  │       LLVM → ARM/x86 (CPU)                             │
  │                                                         │
  │   Auto-tuning: TVM benchmarks many kernel variants     │
  │   on target hardware, picks the fastest.               │
  └──────────────────────────┬──────────────────────────────┘
                             │
                             ▼
  ┌─────────────────────────────────────────────────────────┐
  │ Step 3: Package                                         │
  │                                                         │
  │   Compiled model + runtime → deployable artifact:       │
  │     iOS app (Swift + Metal compiled model)              │
  │     Android app (Java/Kotlin + Vulkan compiled model)   │
  │     Web app (JS + WebGPU compiled model)                │
  │     Desktop binary (Python/C++ + native compiled model) │
  └─────────────────────────────────────────────────────────┘

The key difference from server-side inference:
  TensorRT-LLM: compiles for NVIDIA GPUs only, max performance.
  vLLM: runs on GPUs, uses PyTorch + CUDA.
  MLC-LLM: compiles for ANY device, including non-NVIDIA hardware.
  Trades some peak performance for universality.
```

---

## 3. Why Compilation (Not Just Quantization)

```
Quantization alone isn't enough:

  Llama-3 8B quantized to INT4 = ~4 GB. Fits on a phone!
  But who runs the matrix multiplies?

  On NVIDIA: cuBLAS (hand-tuned CUDA kernels).
  On iPhone: ??? There's no cuBLAS for Apple GPUs.
  On Android: ??? There's no cuBLAS for Qualcomm Adreno.
  In browser: ??? There's no cuBLAS for WebGPU.

  You need OPTIMIZED KERNELS for each platform.
  Options:
    1. Hand-write kernels per platform (llama.cpp approach)
       → Massive engineering effort. Hard to support new models.
    2. COMPILE kernels per platform (MLC-LLM approach)
       → Add a new target by writing a TVM backend.
       → All models automatically work on the new target.

  MLC-LLM's advantage: when a new model architecture comes out
  (new MoE, new attention variant), you just express it in TVM IR.
  The compiler generates optimized kernels for ALL targets.
  No per-platform hand-tuning for each new model.
```

---

## 4. Quantization for Edge

```
Server LLMs use FP8 or INT8 (H100 has FP8 Tensor Cores).
Edge devices need more aggressive quantization:

  ┌────────────────────────────────────────────────────────────┐
  │ Format     │ Model Size │ RAM needed │ Quality  │ Target   │
  ├────────────┼────────────┼────────────┼──────────┼──────────┤
  │ FP16       │ 16 GB (8B) │ ~18 GB     │ Best     │ Laptop   │
  │ INT4 (q4f16)│ 4 GB (8B) │ ~6 GB      │ Good     │ Phone    │
  │ INT3 (q3f16)│ 3 GB (8B) │ ~5 GB      │ Moderate │ Phone    │
  │ INT4 (q4f32)│ 4 GB (8B) │ ~8 GB      │ Good     │ Laptop   │
  └────────────┴────────────┴────────────┴──────────┴──────────┘

  MLC-LLM applies GROUP quantization:
    Weights divided into groups of 32-128 elements.
    Each group gets its own scale + zero-point.
    Better accuracy than per-tensor quantization.

  The compiled kernels FUSE dequantization with matmul:
    Don't dequant to FP16+matmul (wastes memory bandwidth).
    Instead: load INT4 weights, dequant in registers, multiply
    immediately. One fused kernel. This is where TVM helps.
```

---

## 5. WebGPU — LLMs in the Browser

```
MLC-LLM's most unique feature: run LLMs directly in Chrome/Edge.

  How:
    Model weights: downloaded + cached in browser (IndexedDB).
    Compute: WebGPU API → GPU shaders (WGSL language).
    Runtime: compiled TVM runtime in WebAssembly.
    No server needed. All computation on the user's device.

  User experience:
    1. Open web page.
    2. First load: download ~4 GB (INT4 quantized 8B model).
    3. After cached: instant load.
    4. Chat with LLM. All tokens generated locally.
    5. Nothing sent to any server. Full privacy.

  Performance (Llama-3 8B INT4, M2 MacBook, WebGPU):
    ~30-50 tokens/sec decode. Usable for chat.

  Limitations:
    WebGPU is newer, not as optimized as native Metal/CUDA.
    Limited to models that fit in device memory.
    No good for 70B+ models.

  This is what powers the "chat with AI locally" web demos.
```

---

## 6. MLC-LLM vs llama.cpp

```
Both solve "run LLMs on consumer devices." Different approach.

  ┌──────────────────────────────────────────────────────────┐
  │              │ MLC-LLM              │ llama.cpp           │
  ├──────────────┼───────────────────────┼─────────────────────┤
  │ Approach     │ Compiler (TVM)        │ Hand-written C/C++  │
  │ GPU kernels  │ Auto-generated        │ Hand-written Metal/ │
  │              │ (Metal/Vulkan/WebGPU) │ CUDA/Vulkan/SYCL    │
  │ New models   │ Define in IR, compile │ Implement manually  │
  │ Performance  │ Good (auto-tuned)     │ Very good (hand-opt)│
  │ WebGPU       │ Yes (unique strength) │ No (WASM only)      │
  │ Community    │ Smaller               │ Very large           │
  │ Ease of use  │ Medium (compile step) │ Easy (just run)      │
  │ Flexibility  │ Any TVM target        │ Specific backends    │
  ├──────────────┼───────────────────────┼─────────────────────┤
  │ Best for     │ Multi-platform deploy │ Quick local use      │
  │              │ + browser, research   │ maximum community    │
  └──────────────┴───────────────────────┴─────────────────────┘

  llama.cpp wins on: community, ease of use, raw performance.
  MLC-LLM wins on: browser support, compiler-driven portability,
  automatic kernel generation for new architectures.
```

---

## 7. Key Numbers

```
MLC-LLM performance (approximate, varies by hardware):

  Llama-3 8B INT4:
    MacBook M2 Pro (Metal):     ~40-60 tokens/sec decode
    iPhone 15 Pro (Metal):       ~15-25 tokens/sec decode
    Chrome WebGPU (M2):          ~30-50 tokens/sec decode
    NVIDIA RTX 4090 (CUDA):      ~100+ tokens/sec decode

  Llama-3 70B INT4:
    MacBook M2 Ultra 192GB:      ~10-15 tokens/sec decode
    Not feasible on phones (too large even at INT4 ~35 GB)

  Compilation time:
    8B model:  ~10-30 minutes (depends on target + auto-tuning)
    70B model: ~1-2 hours

  Download sizes (INT4 quantized):
    8B model:   ~4 GB
    13B model:  ~7 GB
    70B model:  ~35 GB
```
