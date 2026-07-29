# TokenSpeed — Speed-of-Light LLM Inference Engine for Agentic Workloads

---

## 1. What TokenSpeed Does

```
TokenSpeed (LightSeek Foundation, 2026) solves: how do you serve LLMs
at maximum efficiency for AGENTIC workloads — coding agents, multi-turn
tool use, long-context conversations — on NVIDIA Blackwell (B200)?

  The problem:
    Agentic workloads are different from simple chat:
      - Contexts routinely exceed 50K tokens
      - Conversations span dozens of turns
      - Each turn reuses a massive prefix KV cache
      - Speculative decoding is critical (latency-sensitive)
    Existing engines (vLLM, TRT-LLM) were designed for general serving.
    TokenSpeed is built from first principles for this regime.

  Goals:
    • TensorRT-LLM level performance
    • vLLM level usability
    • Optimized specifically for agentic coding workloads

  Built by a cross-organization team (NVIDIA DevTech, AMD Triton,
  Qwen Inference, Together AI, Mooncake, LongCat, FluentLLM)
  in ~2 months. Preview released May 2026.

  GitHub: https://github.com/lightseekorg/tokenspeed
  Languages: 93% Python, 7% C++
  License: MIT
```

---

## 2. Architecture

```
TokenSpeed has three main subsystems:

  ┌─────────────────────────────────────────────────────────────┐
  │                  Python Modeling Layer                       │
  │  Local SPMD design with I/O placement annotations.          │
  │  Developer specifies parallelism at module boundaries.      │
  │  A lightweight static compiler auto-generates the           │
  │  required collective operations (AllReduce, etc.)           │
  │  during model construction. No manual comms logic.          │
  └───────────────────────────┬─────────────────────────────────┘
                              │
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │              C++ Scheduler (Control Plane)                   │
  │  Finite-state machine design.                               │
  │  Request lifecycle, KV cache resources, and overlap          │
  │  timing are represented through explicit FSM transitions    │
  │  and ownership semantics. Safety enforced by the type       │
  │  system at COMPILE TIME, not runtime convention.            │
  │                                                              │
  │  Separates control plane (C++) from execution plane (Python)│
  │  → safety + correctness in C++, iteration speed in Python.  │
  └───────────────────────────┬─────────────────────────────────┘
                              │
                              ▼
  ┌─────────────────────────────────────────────────────────────┐
  │               Pluggable Kernel Layer                         │
  │  Kernels are a first-class modular subsystem:               │
  │  • Portable public API                                      │
  │  • Centralized registry + selection model                   │
  │  • Extensible plugin mechanism for heterogeneous accelerators│
  │  • Separate from core engine — can swap kernel impls        │
  └─────────────────────────────────────────────────────────────┘

Key design decisions:
  1. Control plane in C++ (correctness via FSM + type system)
  2. Execution plane in Python (fast iteration for researchers)
  3. Kernels decoupled from engine (pluggable, per-accelerator)
  4. SPMD modeling with compiler-generated comms
  5. SMG (Scalable Model Gateway) integration for low-overhead
     CPU-side request entrypoint
```

---

## 3. TokenSpeed MLA — The Core Kernel Innovation

```
MLA (Multi-head Latent Attention) is the attention variant used
by DeepSeek V2/V3/R1 and models like Kimi K2.5.

  MLA compresses KV cache into a lower-dimensional latent space:
    Standard MHA: KV cache = [heads × head_dim] per token
    MLA: KV cache = [latent_dim] per token (much smaller)
    The full KV is reconstructed from the latent on-the-fly.

TokenSpeed built one of the fastest MLA kernels for Blackwell (B200):

  PREFILL kernel optimizations:
    Binary-version prefill kernel with fine-tuned softmax
    implementation using NVIDIA-internal knobs.
    Outperforms TRT-LLM's MLA across all five typical
    prefill workloads for coding agents (prefill with
    long prefix KV cache).

  DECODE kernel optimizations:
    Problem: with MLA, num_heads can be small → Tensor Cores
    underutilized during decode because the M dimension is tiny.

    Solution: fold the query-sequence axis into the head axis.
    Groups q_seqlen and num_heads together to fill the BMM1
    M tile, improving Tensor Core utilization.

    Result: NEARLY HALVES decode latency vs TRT-LLM on typical
    decode workloads with speculative decoding (batch 4/8/16
    with long prefix KV cache).

  ┌──────────────────────────────────────────────────────────┐
  │ MLA decode optimization:                                 │
  │                                                          │
  │ Standard approach:                                       │
  │   Q: [batch × 1 × num_heads × head_dim]                │
  │   BMM1 M-dim = 1 per head → Tensor Cores underutilized  │
  │                                                          │
  │ TokenSpeed approach:                                     │
  │   Fold q_seqlen into heads axis:                         │
  │   Q: [batch × (q_seqlen × num_heads) × head_dim]       │
  │   BMM1 M-dim = q_seqlen × num_heads → better tile fill  │
  │                                                          │
  │ Especially effective with speculative decoding           │
  │ where q_seqlen > 1 (verifying multiple draft tokens).    │
  └──────────────────────────────────────────────────────────┘

  The MLA kernel has been adopted by vLLM (upstream PR merged).
```

---

## 4. Scheduler Design — FSM + Type-Safe KV Management

```
The scheduler is the most architecturally distinctive part.

  Traditional inference schedulers (vLLM, TRT-LLM):
    KV cache management via runtime bookkeeping.
    Block tables, reference counts, manual free lists.
    Correctness depends on getting every code path right.
    Bugs: use-after-free, double-free, race conditions.

  TokenSpeed scheduler:
    KV cache state transfer is modeled as FSM transitions.
    The C++ type system ENFORCES safe resource management
    at compile time.

    ┌──────────────────────────────────────────────────────┐
    │ Request lifecycle as FSM:                            │
    │                                                      │
    │ QUEUED → PREFILLING → DECODING → COMPLETED           │
    │              │            │                           │
    │              └──► PREEMPTED ──► QUEUED (re-enter)    │
    │                                                      │
    │ KV cache ownership transitions:                      │
    │   ALLOCATED → ACTIVE → CACHED → FREED                │
    │                                                      │
    │ Each transition is an explicit type-level operation.  │
    │ Invalid transitions (e.g., use freed KV) are         │
    │ caught by the compiler, not by runtime checks.       │
    └──────────────────────────────────────────────────────┘

  Why this matters for agentic workloads:
    Agents generate long conversations with many turns.
    KV cache must be safely reused across turns.
    With prefix sharing + speculative decoding + preemption,
    the state space is complex. Runtime bugs are hard to find.
    Compile-time safety prevents entire classes of bugs.
```

---

## 5. SPMD Modeling with Compiler-Generated Communication

```
Parallelism is specified declaratively, not manually coded.

  Traditional approach (vLLM, TRT-LLM):
    Developer manually inserts AllReduce / AllGather calls
    in the model code. Must reason about tensor sharding.

  TokenSpeed approach:
    Developer annotates I/O placement at module boundaries.
    A lightweight static compiler generates the required
    collective operations during model construction.

    @placement(input="replicated", output="sharded[tp]")
    def attention(q, k, v):
        # ... compute attention ...
        return output

    The compiler sees: input is replicated, output is sharded.
    It automatically inserts ReduceScatter / AllGather as needed.
    No manual NCCL calls in model code.

  Benefits:
    1. Less error-prone (compiler ensures correctness)
    2. Easier to add new parallelism strategies
    3. Cleaner model code (parallelism is orthogonal to logic)
```

---

## 6. Performance — Kimi K2.5 on B200

```
Benchmarked against TensorRT-LLM on SWE-smith traces
(coding agent traffic, realistic agentic workload):

  Model: Kimi K2.5 (MoE with MLA)
  Hardware: NVIDIA B200
  Workload: SWE-smith coding agent traces (50K+ context)

  Metric: maximize TPM/GPU (throughput) while maintaining
  minimum TPS/User (per-user latency floor, typically 70+ TPS).

  Best config: Attention TP4 + MoE TP4

  ┌──────────────────────────────────────────────────────────┐
  │ TokenSpeed vs TensorRT-LLM (Kimi K2.5, B200):          │
  │                                                          │
  │   Min-latency (batch=1):                                │
  │     TokenSpeed ~9% faster than TRT-LLM                  │
  │                                                          │
  │   Agentic regime (100 TPS/User):                        │
  │     TokenSpeed ~11% higher throughput than TRT-LLM       │
  │                                                          │
  │   TokenSpeed dominates the entire Pareto frontier        │
  │   (no point where TRT-LLM is better at any              │
  │    latency/throughput trade-off).                        │
  └──────────────────────────────────────────────────────────┘

  MLA kernel comparison (vs TRT-LLM MLA on B200):

    PREFILL (long prefix KV cache, coding agent workloads):
      TokenSpeed MLA faster across all 5 typical workloads.

    DECODE (speculative decoding, batch 4/8/16):
      TokenSpeed MLA nearly halves latency vs TRT-LLM.
      The q_seqlen-into-heads folding trick is the key.
```

---

## 7. Where It Fits in the Ecosystem

```
  ┌────────────────────────────────────────────────────────────┐
  │              │ TokenSpeed         │ TRT-LLM    │ vLLM      │
  ├──────────────┼────────────────────┼────────────┼───────────┤
  │ Focus        │ Agentic workloads  │ General    │ General   │
  │ Performance  │ Best on B200       │ Very high  │ High      │
  │ Usability    │ vLLM-like          │ Complex    │ Easy      │
  │ Hardware     │ NVIDIA (B200 opt)  │ NVIDIA     │ Multi-HW  │
  │              │ AMD (in progress)  │ only       │           │
  │ Scheduler    │ C++ FSM, type-safe │ C++        │ Python    │
  │ Kernels      │ Pluggable, modular │ Built-in   │ Built-in  │
  │ MLA support  │ Best (adopted by   │ Good       │ Uses      │
  │              │ vLLM upstream)     │            │ TokenSpeed│
  │ Maturity     │ Preview (May 2026) │ Production │ Production│
  │ License      │ MIT                │ Apache-2.0 │ Apache-2.0│
  ├──────────────┼────────────────────┼────────────┼───────────┤
  │ Best for     │ Coding agents,     │ Max perf,  │ General   │
  │              │ DeepSeek/Kimi MLA  │ fixed cfg  │ serving   │
  │              │ models on Blackwell│ on NVIDIA  │           │
  └──────────────┴────────────────────┴────────────┴───────────┘

  Status (May 2026): PREVIEW release.
    Available now: Kimi K2.5 optimization, MLA kernels.
    Coming soon: Qwen 3.6, DeepSeek V4, MiniMax M2.7,
                 PD disaggregation, MI350 (AMD) support.
    Not yet production-ready — intended to showcase the
    runtime design and technical direction.

  Collaborators: NVIDIA DevTech, AMD Triton, Qwen Inference,
  Together AI, Mooncake, LongCat, FluentLLM, EvalScope.
```
