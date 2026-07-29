# TensorRT-LLM — NVIDIA's High-Performance LLM Inference Engine

---

## 1. What TensorRT-LLM Does

```
TensorRT-LLM (NVIDIA) solves: how do you serve LLMs at maximum
speed on NVIDIA GPUs by squeezing out every drop of hardware perf?

  The GPU has theoretical peak FLOPS and memory bandwidth.
  Most serving frameworks reach 30-50% of peak.
  TensorRT-LLM targets 70-80%+ by:
    1. Compiling models into optimized GPU kernels (TensorRT engine)
    2. Hand-written CUDA kernels for attention, GEMM, etc.
    3. In-flight batching (continuous batching)
    4. Quantization (FP8, INT8, INT4, AWQ, GPTQ, SmoothQuant)
    5. Paged KV cache (like vLLM)
    6. Multi-GPU / multi-node inference (TP + PP)
    7. Speculative decoding

  It's NOT a Python serving framework — it's a C++ inference engine
  with a Python build API. You construct the model graph in Python,
  TensorRT compiles it into a .engine file, then the C++ runtime
  executes it.

  Used by: NVIDIA Triton, many production deployments on NVIDIA GPUs.
  Targets: Llama, GPT, Falcon, Mistral, Mixtral, Gemma, Qwen,
           BLOOM, ChatGLM, Phi, and most popular architectures.

  GitHub: https://github.com/NVIDIA/TensorRT-LLM
```

---

## 2. Architecture Overview

```
TensorRT-LLM has three main layers:

  ┌─────────────────────────────────────────────────────────────┐
  │                   Python Build API                          │
  │  Define model graph using tensorrt_llm.Module               │
  │  (like PyTorch nn.Module but builds a TensorRT network)     │
  └───────────────────────────┬─────────────────────────────────┘
                              │ build()
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │                TensorRT Compiler                            │
  │  - Fuses operations (LayerNorm+GEMM, attention patterns)    │
  │  - Selects optimal CUDA kernels per layer                   │
  │  - Inserts quantization/dequantization nodes                │
  │  - Produces a serialized .engine file                       │
  └───────────────────────────┬─────────────────────────────────┘
                              │ load engine
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │              C++ Runtime (Executor API)                      │
  │  - Manages GPU memory, KV cache, scheduling                 │
  │  - In-flight batching (continuous batching)                  │
  │  - Multi-GPU communication (NCCL)                           │
  │  - Streams requests, returns tokens                         │
  └─────────────────────────────────────────────────────────────┘

The workflow:
  1. OFFLINE: Build the engine (minutes to hours).
     - Load a HuggingFace checkpoint.
     - Construct TRT-LLM model graph.
     - TensorRT optimizes + compiles → .engine file.
     - Engine is specific to: GPU arch, max batch, max seq len,
       quantization config, TP/PP layout.

  2. ONLINE: Run inference with the engine.
     - Load .engine into the C++ runtime.
     - Submit requests via Executor API.
     - Runtime handles batching, scheduling, KV cache, decoding.
     - Returns streamed tokens.

  Typical build command:
    trtllm-build \
      --checkpoint_dir ./llama-70b-ckpt \
      --output_dir ./llama-70b-engine \
      --gemm_plugin float16 \
      --max_batch_size 64 \
      --max_input_len 4096 \
      --max_seq_len 8192 \
      --tp_size 8 \
      --pp_size 1
```

---

## 3. Why Compilation Matters — TensorRT Under the Hood

```
PyTorch: eager execution. Each op dispatches a CUDA kernel.
  Linear → cuBLAS GEMM kernel
  LayerNorm → separate kernel
  Activation → separate kernel
  Each kernel: launch overhead + memory read/write.

  PyTorch for Llama-70B forward pass:
    hundreds of kernel launches per token
    each kernel reads from and writes to GPU global memory
    memory bandwidth becomes the bottleneck

TensorRT: graph compilation. Fuses ops into fewer, bigger kernels.

  Before fusion:
    LayerNorm → kernel 1 → write to mem → read from mem →
    Linear    → kernel 2 → write to mem → read from mem →
    GELU      → kernel 3 → write to mem

  After fusion:
    [LayerNorm + Linear + GELU] → single kernel
    One read, one write. 3x less memory traffic.

  Key optimizations TensorRT applies:
    1. LAYER FUSION: combine adjacent ops into one kernel.
       - Conv + BatchNorm + ReLU → single kernel
       - LayerNorm + projection → single kernel
       - QKV projections fused into one big GEMM

    2. KERNEL AUTO-TUNING: for each GEMM shape, TensorRT
       benchmarks multiple kernel implementations and picks
       the fastest one for THIS specific GPU.
       cuBLAS has 50+ GEMM kernels for different shapes.
       TensorRT tries them all and picks the winner.

    3. PRECISION CALIBRATION: convert FP32/FP16 ops to lower
       precision (FP8, INT8) where accuracy loss is minimal.
       Measures activation ranges, chooses scaling factors.

    4. MEMORY PLANNING: pre-allocates all intermediate buffers.
       No malloc/free during inference. Zero allocation overhead.

  Result:
    PyTorch eager: ~60% GPU utilization (memory-bound)
    TensorRT compiled: ~80%+ GPU utilization
    1.5-3x faster for the same model on the same hardware.
```

---

## 4. Attention Kernels — The Core Performance Engine

```
Self-attention is the most latency-critical op in LLM inference.
TensorRT-LLM ships custom attention kernels, not generic ones.

Two phases of LLM inference:

  PREFILL (context/prompt processing):
    Process all input tokens at once.
    COMPUTE-BOUND: large matrix multiplies.
    Q: [batch × seq_len × hidden]  ← many tokens
    K: [batch × seq_len × hidden]
    Attention: [batch × heads × seq_len × seq_len]  ← huge
    Uses FlashAttention-2 style fused kernel.

  DECODE (token generation):
    Generate one token at a time.
    MEMORY-BOUND: reading the entire KV cache for one query.
    Q: [batch × 1 × hidden]  ← just one new token
    K: [batch × (seq_len+t) × hidden]  ← full KV cache
    Uses custom "Masked Multi-Head Attention" (MMHA) kernel.

  ┌──────────────────────────────────────────────────────────┐
  │ PREFILL kernel (FlashAttention style):                   │
  │                                                          │
  │   for each block of Q rows:                              │
  │     for each block of K columns:                         │
  │       load Q_block, K_block, V_block into SRAM           │
  │       compute attention_block = softmax(Q_block @ K_block│
  │       accumulate output_block += attention_block @ V_block│
  │                                                          │
  │   Never materializes the full attention matrix in HBM.   │
  │   Memory: O(seq_len) instead of O(seq_len²).            │
  │   Compute: same. Just avoids unnecessary memory traffic. │
  └──────────────────────────────────────────────────────────┘

  ┌──────────────────────────────────────────────────────────┐
  │ DECODE kernel (MMHA):                                    │
  │                                                          │
  │   Only ONE query vector per request.                     │
  │   Must read entire KV cache from memory.                 │
  │   Bottleneck: memory bandwidth, not compute.             │
  │                                                          │
  │   Optimization: each CUDA thread block handles one       │
  │   attention head. Threads cooperatively load KV cache    │
  │   from HBM, compute dot products, reduce.               │
  │                                                          │
  │   With GQA (grouped-query attention):                    │
  │     Multiple query heads share one KV head.              │
  │     Less KV cache to read → faster decode.               │
  │     Llama-3 70B: 64 Q heads, 8 KV heads (GQA ratio 8).  │
  │     KV cache read reduced by 8× vs MHA.                 │
  └──────────────────────────────────────────────────────────┘

  XQA (Cross-Query Attention) kernel — TRT-LLM specific:
    Further optimizes GQA decode.
    Multiple query heads that share a KV group are processed
    by the SAME thread block, reusing KV data in shared memory.
    Avoids redundant KV cache reads.
    ~1.5x faster decode for GQA models.
```

---

## 5. In-Flight Batching

```
Static batching:
  Wait for a batch of N requests → process all together → return all.
  Problem: short requests wait for long ones.

  Request 1: "Hi"           → 5 tokens
  Request 2: "Write essay"  → 500 tokens
  Request 3: "2+2?"         → 3 tokens

  Static batch: all three wait until request 2 finishes 500 tokens.
  Requests 1 and 3 have terrible latency.

In-flight batching (TRT-LLM calls it "inflight batching"):
  Requests can JOIN and LEAVE the batch at any time.

  Step 1: [Req1, Req2, Req3] → generate 1 token each
  Step 2: [Req1, Req2, Req3] → generate 1 token each
  Step 3: [Req1✓, Req2, Req3✓] → Req1 and Req3 done! Remove them.
  Step 4: [Req4, Req2, Req5] → new Req4 and Req5 join immediately
  Step 5: [Req4, Req2, Req5] → continue...

  ┌─────────────────────────────────────────────────────────┐
  │ Scheduler loop (each iteration):                        │
  │                                                         │
  │   1. Check finished requests → return results, free KV  │
  │   2. Check queue for new requests                       │
  │   3. If capacity available:                             │
  │      - Run PREFILL for new requests (chunked if needed) │
  │      - Run DECODE for existing requests                 │
  │   4. Execute one forward pass with mixed batch          │
  │      (some requests prefilling, others decoding)        │
  │   5. Repeat                                             │
  │                                                         │
  │ Chunked prefill:                                        │
  │   Long prompts split into chunks.                       │
  │   Each iteration processes one chunk + decode tokens.   │
  │   Prevents long prompts from stalling decode requests.  │
  └─────────────────────────────────────────────────────────┘

  This is the same idea as "continuous batching" in vLLM.
  TRT-LLM implements it in C++ for lower overhead.
```

---

## 6. Paged KV Cache

```
Same idea as vLLM's PagedAttention:

  Each request's KV cache is stored in pages (blocks).
  Block size: typically 64 or 128 tokens.
  Blocks are allocated from a pre-allocated pool.

  ┌─────────────────────────────────────────────────────────┐
  │ KV Cache Pool (pre-allocated at engine build time)      │
  │                                                         │
  │ [Block 0] [Block 1] [Block 2] [Block 3] [Block 4] ...  │
  │                                                         │
  │ Request A (150 tokens): Block 0 → Block 1 → Block 2    │
  │ Request B (80 tokens):  Block 3 → Block 4              │
  │ Request C (200 tokens): Block 5 → Block 6 → Block 7    │
  │ Free list: Block 8, Block 9, Block 10, ...              │
  │                                                         │
  │ Blocks are not contiguous in memory.                    │
  │ Attention kernel uses a block table (indirection) to    │
  │ look up which physical blocks belong to which request.  │
  └─────────────────────────────────────────────────────────┘

  Why paging matters for TRT-LLM specifically:
    TensorRT pre-plans memory layout at build time.
    Without paging: must allocate max_batch × max_seq_len KV cache.
    For 64 requests × 8192 tokens × 70B model: ~160 GB.
    Doesn't fit even on 8× H100s.

    With paging: allocate blocks on demand.
    Actual usage: ~20-40% of theoretical max.
    Same 8× H100s can now serve the workload.
```

---

## 7. Quantization

```
TensorRT-LLM has deep quantization support:

  ┌──────────────────────────────────────────────────────────┐
  │ Format  │ Bits │ Speedup │ Quality Loss │ Use Case      │
  ├─────────┼──────┼─────────┼──────────────┼───────────────┤
  │ FP16    │ 16   │ 1x      │ None         │ Baseline      │
  │ BF16    │ 16   │ 1x      │ None         │ Baseline      │
  │ FP8     │ 8    │ ~2x     │ Minimal      │ H100/H200     │
  │ INT8 SQ │ 8    │ ~1.8x   │ Small        │ A100/H100     │
  │ INT8 WO │ 8    │ ~1.5x   │ Small        │ Weight-only   │
  │ INT4 AWQ│ 4    │ ~2.5x   │ Moderate     │ Cost-optimize │
  │ INT4 GPT│ 4    │ ~2.5x   │ Moderate     │ Cost-optimize │
  │ FP4     │ 4    │ ~3x     │ Moderate     │ Blackwell     │
  └──────────┴──────┴─────────┴──────────────┴───────────────┘

  FP8 on H100/H200 (the sweet spot):
    H100 has dedicated FP8 Tensor Cores: 1979 TFLOPS (vs 989 FP16).
    FP8 weights + FP8 activations → 2x compute throughput.
    Also 2x less memory bandwidth (read half the bytes).
    Quality barely degrades for most models.

    How FP8 works in TRT-LLM:
      1. Calibration: run a few hundred samples through the model.
         Measure the distribution of activations per layer.
      2. Compute scaling factors: max(abs(tensor)) → scale to FP8 range.
         Per-tensor or per-channel scaling.
      3. At inference: weights stored as FP8, activations quantized
         on-the-fly, GEMM in FP8, accumulate in FP32, output FP16.

  SmoothQuant (INT8):
    Problem: activations have outliers. Quantizing directly loses info.
    Solution: "smooth" the activations by scaling them down,
    and scale the weights up by the same factor.
    Mathematically: Y = (X / s) @ (W × s) = X @ W.
    After smoothing, both X and W have similar ranges → INT8 works.

  Weight-Only Quantization (INT4 AWQ/GPTQ):
    Only quantize WEIGHTS to INT4. Activations stay FP16.
    During GEMM: dequantize INT4 → FP16, then multiply.
    Saves memory (model fits on fewer GPUs) and bandwidth
    (read 4x fewer bytes for weights).
    Compute throughput unchanged (still FP16 multiply).
    Main benefit: decode is memory-bound, so reading less = faster.
```

---

## 8. Multi-GPU Inference (TP + PP)

```
TensorRT-LLM supports Tensor Parallelism and Pipeline Parallelism
for models too large for one GPU.

  Tensor Parallelism (TP):
    Same approach as Megatron: split weight matrices across GPUs.
    Column-parallel for first linear, row-parallel for second.
    AllReduce via NCCL over NVLink.

    Llama-70B on 8× H100:
      TP=8. Each GPU holds 1/8 of each layer.
      Weight per GPU: ~17 GB (140 GB / 8).
      Leaves ~60 GB for KV cache + activations.

  Pipeline Parallelism (PP):
    Split layers across nodes.
    Less common for inference (adds latency per stage).
    Used for very large models (405B+) or when TP alone isn't enough.

    Llama-405B on 16× H100 (2 nodes):
      TP=8, PP=2.
      Node 0: first 63 layers (TP=8 within node).
      Node 1: last 63 layers + embedding + head (TP=8 within node).

  ┌──────────────────────────────────────────────────────────┐
  │          TP vs PP for inference                          │
  │                                                          │
  │  TP:                                                     │
  │    + Lower latency (all GPUs work on every token)        │
  │    - More AllReduces (NVLink only, not across nodes)     │
  │    - Limited to GPUs within NVLink domain (usually 8)    │
  │                                                          │
  │  PP:                                                     │
  │    + Works across nodes (InfiniBand)                     │
  │    + Less communication (just activations between stages)│
  │    - Higher latency (stages are sequential)              │
  │    - Pipeline bubble (but less relevant for inference    │
  │      than training — no backward pass)                   │
  │                                                          │
  │  For inference: maximize TP first (up to 8), then PP.    │
  └──────────────────────────────────────────────────────────┘
```

---

## 9. Speculative Decoding

```
Autoregressive decoding is slow:
  Generate one token → read entire model → repeat.
  Each token requires a full model forward pass.
  Latency = N_tokens × time_per_token.

Speculative decoding accelerates this:

  ┌──────────────────────────────────────────────────────────┐
  │ Draft model: small, fast (e.g., 1B params)              │
  │ Target model: large, accurate (e.g., 70B params)        │
  │                                                          │
  │ Step 1: Draft model generates K tokens (fast).          │
  │   "The capital of France is Paris, which" (K=5 tokens)  │
  │                                                          │
  │ Step 2: Target model VERIFIES all K tokens in ONE pass. │
  │   Forward pass with all K draft tokens as input.         │
  │   Check: does 70B agree with 1B's predictions?          │
  │                                                          │
  │ Step 3: Accept matching tokens, reject + re-sample rest. │
  │   If 70B agrees with first 4: accept 4 tokens at once.  │
  │   Reject 5th, sample correct 5th from 70B distribution. │
  │                                                          │
  │ Result: generated 4-5 tokens in the time of ~2 passes   │
  │ (one fast draft + one target verify), instead of 5       │
  │ expensive target passes. ~2-3x speedup for decode.       │
  └──────────────────────────────────────────────────────────┘

  TRT-LLM supports:
    - External draft model (separate small model)
    - Medusa: add small "Medusa heads" on top of the target model
      that predict multiple future tokens. No separate draft model.
    - Eagle: similar to Medusa but with autoregressive draft heads.
    - Lookahead decoding: Jacobi iteration based approach.

  When speculative decoding helps most:
    - Low batch size (GPU underutilized, memory-bound)
    - High acceptance rate (draft model is good)
    - Latency-sensitive workloads (chat, interactive)

  When it doesn't help:
    - High batch size (GPU already compute-saturated)
    - Low acceptance rate (draft model too different)
    - Throughput-optimized workloads (just increase batch size)
```

---

## 10. Executor API — The Runtime Interface

```
The Executor API is the C++ runtime that manages everything:

  ┌──────────────────────────────────────────────────────────┐
  │ Application                                              │
  │    │                                                     │
  │    ▼                                                     │
  │ Executor.enqueue_request(prompt, params)                 │
  │    │                                                     │
  │    ▼                                                     │
  │ ┌──────────────────────────────────────────────────────┐ │
  │ │ Request Queue                                        │ │
  │ │ [Req1: "Explain X", max_tokens=200]                  │ │
  │ │ [Req2: "Summarize Y", max_tokens=100]                │ │
  │ └─────────────────────┬────────────────────────────────┘ │
  │                       ▼                                  │
  │ ┌──────────────────────────────────────────────────────┐ │
  │ │ Scheduler                                            │ │
  │ │ - Picks which requests to prefill vs decode          │ │
  │ │ - Manages KV cache allocation (paged)                │ │
  │ │ - Applies chunked prefill if prompt is too long      │ │
  │ │ - Handles preemption if memory is tight              │ │
  │ └─────────────────────┬────────────────────────────────┘ │
  │                       ▼                                  │
  │ ┌──────────────────────────────────────────────────────┐ │
  │ │ TensorRT Engine Execution                            │ │
  │ │ - Runs the compiled .engine on the GPU(s)            │ │
  │ │ - Returns logits                                     │ │
  │ └─────────────────────┬────────────────────────────────┘ │
  │                       ▼                                  │
  │ ┌──────────────────────────────────────────────────────┐ │
  │ │ Sampling / Beam Search                               │ │
  │ │ - Top-k, top-p, temperature, repetition penalty      │ │
  │ │ - Beam search with length penalty                    │ │
  │ └─────────────────────┬────────────────────────────────┘ │
  │                       ▼                                  │
  │ Executor.get_responses() → stream tokens to application  │
  └──────────────────────────────────────────────────────────┘

  Python usage:
    import tensorrt_llm
    from tensorrt_llm import Executor

    executor = Executor(
        model_path="./llama-70b-engine",
        executor_config=ExecutorConfig(
            max_beam_width=1,
            batching_type=BatchingType.INFLIGHT,
        )
    )

    request = Request(
        input_token_ids=[...],
        max_tokens=200,
        sampling_config=SamplingConfig(top_k=1),
    )
    executor.enqueue_request(request)

    # Stream responses
    for response in executor.await_responses():
        print(response.output_token_ids)
```

---

## 11. TRT-LLM vs vLLM — When to Use Which

```
  ┌────────────────────────────────────────────────────────────┐
  │              │ TensorRT-LLM          │ vLLM                │
  ├──────────────┼────────────────────────┼─────────────────────┤
  │ Language     │ C++ core, Python API   │ Python + C++/CUDA   │
  │ Compilation  │ Ahead-of-time (engine) │ JIT (torch.compile) │
  │ Flexibility  │ Must rebuild for       │ Dynamic, any HF     │
  │              │ config changes         │ model loads directly │
  │ Performance  │ 10-30% faster (peak)   │ Very fast, close     │
  │ Quantization │ Best FP8/INT4 support  │ Good, improves fast  │
  │ Hardware     │ NVIDIA only            │ NVIDIA + AMD + TPU   │
  │ Ease of use  │ Complex build process  │ Simple: 1 command    │
  │ Model support│ Curated model list     │ Any HF model         │
  │ Community    │ NVIDIA-driven          │ Large open community │
  │ Production   │ Triton integration     │ Standalone or Ray    │
  ├──────────────┼────────────────────────┼─────────────────────┤
  │ Best for     │ Max perf on NVIDIA,    │ Flexibility, rapid   │
  │              │ fixed model config,    │ iteration, multi-HW, │
  │              │ FP8 on H100            │ ease of deployment   │
  └──────────────┴────────────────────────┴─────────────────────┘

  In practice: many teams start with vLLM (easier), switch to
  TRT-LLM when they need the last 10-30% of performance and are
  committed to NVIDIA hardware.

  Both support: continuous batching, paged KV cache, TP/PP,
  speculative decoding, prefix caching.
```

---

## 12. Key Numbers

```
TensorRT-LLM performance (H100 80GB, BF16/FP8):

  Llama-3 8B, 1× H100, FP8:
    Prefill: ~40K tokens/sec
    Decode:  ~3K tokens/sec (batch=1)
    Decode:  ~25K tokens/sec (batch=64)

  Llama-3 70B, 8× H100 (TP=8), FP8:
    Prefill: ~25K tokens/sec
    Decode:  ~800 tokens/sec (batch=1)
    Decode:  ~12K tokens/sec (batch=64)

  Llama-3 405B, 16× H100 (TP=8, PP=2), FP8:
    Prefill: ~12K tokens/sec
    Decode:  ~400 tokens/sec (batch=1)
    Decode:  ~6K tokens/sec (batch=64)

  FP8 vs FP16 on H100:
    ~1.5-2x faster prefill (compute-bound, FP8 doubles FLOPS)
    ~1.3-1.5x faster decode (memory-bound, FP8 halves reads)

  TRT-LLM vs vLLM (same hardware, same model):
    Typically 10-30% higher throughput for TRT-LLM.
    Gap is narrowing as vLLM improves.

  Engine build time:
    8B model:   ~5-10 minutes
    70B model:  ~30-60 minutes
    405B model: ~2-4 hours
    Engine must be rebuilt for different batch/seq/quant configs.
```

---

## 13. Triton Inference Server Integration

```
In production, TRT-LLM is typically deployed behind NVIDIA Triton:

  ┌──────────────┐     ┌──────────────────────────────┐
  │  Clients     │────▶│  Triton Inference Server      │
  │  (HTTP/gRPC) │     │                               │
  └──────────────┘     │  ┌─────────────────────────┐  │
                       │  │ TRT-LLM Backend          │  │
                       │  │ - Loads .engine files     │  │
                       │  │ - Manages Executor        │  │
                       │  │ - Handles request routing │  │
                       │  └─────────────────────────┘  │
                       │                               │
                       │  Features:                    │
                       │  - Multi-model serving        │
                       │  - Request queuing            │
                       │  - Health checks              │
                       │  - Metrics (Prometheus)       │
                       │  - Streaming responses (SSE)  │
                       │  - Dynamic batching           │
                       └──────────────────────────────┘

  Triton handles the serving infra (HTTP, load balancing, metrics).
  TRT-LLM handles the actual inference execution.
  Together they form NVIDIA's production inference stack.
```
