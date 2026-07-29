# DeepEP — Communication Library for MoE Models

---

## 1. What DeepEP Does

```
DeepEP is an open-source communication library from DeepSeek,
purpose-built for Mixture-of-Experts (MoE) model training and inference.

The core problem it solves:

  In MoE models (like DeepSeek-V3, Mixtral), each token is routed
  to a SUBSET of experts. Different tokens go to different experts.
  But experts live on different GPUs.

  So you need ALL-TO-ALL communication:
    GPU 0 has tokens for experts on GPU 1, 2, 3, ...
    GPU 1 has tokens for experts on GPU 0, 2, 3, ...
    Every GPU must SEND tokens to every other GPU (potentially).

  This is fundamentally different from AllReduce:
    AllReduce: every GPU sends THE SAME shaped data.
    MoE all-to-all: every GPU sends DIFFERENT amounts to each peer.
                    (some experts are "hot," some are "cold")

  NCCL's generic all-to-all doesn't handle this well because:
    1. Token counts per expert are IMBALANCED (some experts get 10x more)
    2. The pattern changes EVERY layer, EVERY step
    3. MoE needs to overlap communication with expert computation
    4. Memory is precious — can't pre-allocate for worst case

  DeepEP handles all of this with optimized GPU kernels.

  ncclAlltoAll()           → treats all peers equally, wastes bandwidth
  DeepEP dispatch/combine  → adapts to actual token routing, overlaps compute
```

---

## 2. The MoE Communication Pattern

```
MoE forward pass (simplified):

  Input tokens: [T0, T1, T2, T3, T4, T5, T6, T7]
                 distributed across 4 GPUs

  Step 1: GATE (router) decides which expert each token goes to.

    GPU 0 has [T0, T1]:   T0 → Expert 3 (on GPU 3)
                          T1 → Expert 1 (on GPU 1)
    GPU 1 has [T2, T3]:   T2 → Expert 0 (on GPU 0)
                          T3 → Expert 2 (on GPU 2)
    GPU 2 has [T4, T5]:   T4 → Expert 1 (on GPU 1)
                          T5 → Expert 0 (on GPU 0)
    GPU 3 has [T6, T7]:   T6 → Expert 2 (on GPU 2)
                          T7 → Expert 3 (on GPU 3)

  Step 2: DISPATCH — send tokens to the GPU that owns their expert.
          This is an all-to-all with IRREGULAR sizes.

    GPU 0 → GPU 1: [T1]        GPU 2 → GPU 0: [T5]
    GPU 0 → GPU 3: [T0]        GPU 2 → GPU 1: [T4]
    GPU 1 → GPU 0: [T2]        GPU 3 → GPU 2: [T6]
    GPU 1 → GPU 2: [T3]        GPU 3 → GPU 3: [T7] (local)

  Step 3: EXPERT COMPUTE — each GPU runs its expert on received tokens.

    GPU 0 (Expert 0): process [T2, T5]  → [T2', T5']
    GPU 1 (Expert 1): process [T1, T4]  → [T1', T4']
    GPU 2 (Expert 2): process [T3, T6]  → [T3', T6']
    GPU 3 (Expert 3): process [T0, T7]  → [T0', T7']

  Step 4: COMBINE — send results BACK to the originating GPU.
          Reverse of dispatch.

    GPU 0 → GPU 1: [T2']       GPU 2 → GPU 1: [T3']
    GPU 0 → GPU 2: [T5']       GPU 2 → GPU 3: [T6']
    GPU 1 → GPU 0: [T1']       GPU 3 → GPU 0: [T0']
    GPU 1 → GPU 2: [T4']       GPU 3 → GPU 3: [T7'] (local)

  After combine, each GPU has the expert outputs for its original tokens:
    GPU 0: [T0', T1']
    GPU 1: [T2', T3']
    GPU 2: [T4', T5']
    GPU 3: [T6', T7']

  The DISPATCH and COMBINE are the two all-to-all operations.
  DeepEP optimizes both.
```

---

## 3. Two Kernels: Normal and Low-Latency

```
DeepEP provides TWO distinct communication kernels:

┌─────────────────────────────────────────────────────────────────┐
│  NORMAL KERNEL — for TRAINING and prefill                      │
│                                                                 │
│  Characteristics:                                               │
│    - High throughput, optimized for large token batches         │
│    - Uses RDMA (InfiniBand) for inter-node communication       │
│    - NVLink for intra-node transfers                           │
│    - Pipelining: overlaps communication with expert compute    │
│    - Best for: large batch sizes, high arithmetic intensity    │
│                                                                 │
│  Mechanism:                                                     │
│    1. GPU packs tokens destined for each peer into buffers     │
│    2. RDMA write sends buffers directly to peer GPU memory     │
│    3. Receiving GPU unpacks and feeds to local expert           │
│    4. All done with CUDA kernels — no CPU involvement          │
│                                                                 │
│  Bandwidth focus: maximize GB/s across InfiniBand links.       │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  LOW-LATENCY KERNEL — for INFERENCE (decode phase)             │
│                                                                 │
│  Characteristics:                                               │
│    - Minimal latency, optimized for small token counts         │
│    - Uses RDMA but with different strategies:                  │
│      - Smaller transfer granularity                            │
│      - Eager sending (don't wait to fill buffers)              │
│      - Reduced synchronization overhead                        │
│    - Best for: decode-time (1 token per request, many requests)│
│                                                                 │
│  Why different from normal?                                     │
│    During decode, each request generates only 1 token.         │
│    Even with batching, total tokens << training batch.         │
│    Bandwidth is NOT the bottleneck — LATENCY is.              │
│    Every microsecond of dispatch/combine delay adds to TTFT.   │
│                                                                 │
│  Latency focus: minimize μs per all-to-all operation.         │
└─────────────────────────────────────────────────────────────────┘

  Training/Prefill:  thousands of tokens → normal kernel (throughput)
  Decode:            tens of tokens       → low-latency kernel (latency)
```

---

## 4. Core Design: GPU-Initiated RDMA

```
Traditional NCCL flow:
  1. CPU calls ncclAlltoAll()
  2. NCCL library (on CPU) sets up transfers
  3. GPU DMAs data over network
  4. CPU polls for completion

DeepEP flow (GPU-initiated):
  1. GPU kernel directly issues RDMA writes
  2. No CPU involvement in the data path
  3. GPU kernel tracks completion via GPU-side flags

Why this matters:

  ┌─────────────────────────────────────────────────────────────┐
  │  CPU-driven (NCCL):                                        │
  │                                                             │
  │  CPU ──launch──► GPU kernel (pack data)                    │
  │  CPU ◄──done──── GPU signals completion                    │
  │  CPU ──issue──► RDMA transfer                              │
  │  CPU ──poll───► wait for completion                        │
  │  CPU ──launch──► GPU kernel (unpack)                       │
  │                                                             │
  │  Each CPU involvement adds ~5-10 μs of overhead.           │
  │  Multiple round-trips CPU↔GPU.                             │
  │                                                             │
  ├─────────────────────────────────────────────────────────────┤
  │  GPU-initiated (DeepEP):                                   │
  │                                                             │
  │  GPU kernel: pack → RDMA write → wait → unpack            │
  │              (all in one kernel, no CPU round-trips)        │
  │                                                             │
  │  GPU directly programs the NIC via GDRCopy/GPU-direct.     │
  │  Completion flags are in GPU memory — no CPU polling.       │
  │                                                             │
  │  For decode (small messages): 20-30% lower latency.        │
  └─────────────────────────────────────────────────────────────┘
```

---

## 5. Communication-Computation Overlap

```
A key optimization: while tokens are being dispatched to remote
experts, local experts can start computing on tokens that are
already available locally.

Without overlap:
  |--- dispatch all-to-all ---|--- expert compute ---|--- combine all-to-all ---|
  Total time = T_dispatch + T_compute + T_combine

With DeepEP overlap:
  |--- dispatch ---|
       |--- expert compute (local tokens first) ---|
                                      |--- combine ---|
  Total time ≈ max(T_dispatch, T_compute) + tail of combine

  How it works:
  1. Dispatch starts sending tokens to remote GPUs.
  2. Tokens routed to LOCAL expert are immediately available.
  3. Local expert begins computing while remote tokens are in flight.
  4. As remote tokens arrive, they're queued for expert compute.
  5. Expert compute overlaps with remaining dispatch traffic.
  6. Combine (sending results back) starts as soon as first expert finishes.

  For DeepSeek-V3 (256 experts, top-8 routing):
    ~3% of tokens go to local expert → processed immediately
    ~97% arrive from network → processed as they arrive
    Net effect: communication is nearly hidden behind computation.
```

---

## 6. Handling Expert Imbalance

```
MoE models have a fundamental challenge: expert load imbalance.

  Some experts are "hot" — they get many tokens.
  Some experts are "cold" — they get few tokens.

  Example (8 experts across 8 GPUs, 1024 tokens total):
    Expert 0: 200 tokens   ← hot
    Expert 1: 180 tokens   ← hot
    Expert 2: 50 tokens    ← cold
    Expert 3: 130 tokens
    Expert 4: 120 tokens
    Expert 5: 40 tokens    ← cold
    Expert 6: 150 tokens   ← hot
    Expert 7: 154 tokens

  Problem: GPU 0 receives 200 tokens, GPU 5 receives only 40.
  GPU 5 finishes 5x faster and sits idle.
  The all-to-all also becomes unbalanced — GPU 0's NIC is saturated.

  DeepEP handles imbalance:

  1. DYNAMIC BUFFER ALLOCATION
     Don't pre-allocate fixed-size receive buffers.
     DeepEP uses a token-count exchange phase:
       - Each GPU announces how many tokens it will send to each peer
       - Receive buffers are allocated based on actual counts
       - No wasted memory for "cold" experts

  2. FLOW CONTROL
     If a GPU's receive buffer fills up (hot expert), DeepEP
     applies backpressure rather than dropping or crashing.
     Senders slow down for hot destinations.

  3. WORKS WITH AUXILIARY LOSS
     Training uses an auxiliary load-balancing loss to keep
     experts roughly balanced. DeepEP handles the residual
     imbalance that the loss doesn't fully eliminate.
```

---

## 7. Memory Management

```
MoE all-to-all is memory-hungry:

  Each GPU must buffer:
    - Outgoing tokens for each peer (dispatch send buffers)
    - Incoming tokens from each peer (dispatch recv buffers)
    - Outgoing expert outputs (combine send buffers)
    - Incoming expert outputs (combine recv buffers)

  For N GPUs, E experts, B tokens, D hidden dim:
    Naive allocation: 4 × N × (B/E) × D × sizeof(dtype)
    With imbalance headroom: even more

  DeepEP's memory strategy:

  ┌─────────────────────────────────────────────────────────────┐
  │  1. SHARED MEMORY POOL                                      │
  │     Dispatch send buffers are freed as data is transmitted. │
  │     Combine reuses the SAME memory pool.                    │
  │     Dispatch and combine don't overlap → share buffers.     │
  │                                                             │
  │  2. LOW-PRECISION COMMUNICATION                             │
  │     Tokens can be quantized to FP8 before dispatch.         │
  │     2x less memory, 2x less bandwidth.                      │
  │     Dequantized on the receiving side before expert compute.│
  │     DeepSeek-V3 uses FP8 dispatch → minimal quality loss.  │
  │                                                             │
  │  3. ON-THE-FLY PACKING                                      │
  │     GPU kernel packs tokens into contiguous buffers         │
  │     as part of the dispatch operation — no separate copy.   │
  │     Reduces peak memory by avoiding intermediate buffers.   │
  └─────────────────────────────────────────────────────────────┘
```

---

## 8. Architecture: How DeepEP Fits in the Stack

```
  ┌─────────────────────────────────────┐
  │          MoE Model (PyTorch)         │
  │  (DeepSeek-V3, Mixtral, etc.)       │
  ├─────────────────────────────────────┤
  │         DeepEP Python API           │
  │  dispatch(tokens, expert_ids)       │
  │  combine(expert_outputs, metadata)  │
  ├─────────────────────────────────────┤
  │        DeepEP CUDA Kernels          │
  │  - Token packing / unpacking        │
  │  - FP8 quantization / dequant       │
  │  - GPU-initiated RDMA writes        │
  │  - Completion flag management       │
  ├─────────────────────────────────────┤
  │     Communication Backend           │
  │  - InfiniBand verbs (ibverbs)       │
  │  - NVLink (intra-node)              │
  │  - GDRCopy (GPU-direct RDMA)        │
  ├─────────────────────────────────────┤
  │         Hardware                     │
  │  NVLink ─── NVSwitch ─── IB NIC    │
  └─────────────────────────────────────┘

  Key dependencies:
    - CUDA 12+
    - NCCL (used for bootstrap / initial setup, NOT for data path)
    - ibverbs (InfiniBand user-space library)
    - GDRCopy or nvidia-peermem (for GPU-direct RDMA)
    - PyTorch (Python bindings)
```

---

## 9. Performance: DeepEP vs NCCL All-to-All

```
Reported numbers from DeepSeek (DeepSeek-V3 training):

  Setup: 256 GPUs (32 nodes × 8 H800 GPUs per node)
         256 experts, top-8 routing
         Token hidden dim: 7168

  ┌──────────────────────┬───────────────┬───────────────┐
  │  Metric              │  NCCL a2a     │  DeepEP       │
  ├──────────────────────┼───────────────┼───────────────┤
  │  Dispatch latency    │  ~1.2 ms      │  ~0.5 ms      │
  │  Combine latency     │  ~1.1 ms      │  ~0.5 ms      │
  │  Bandwidth util.     │  ~45%         │  ~78%         │
  │  Overlap efficiency  │  ~30%         │  ~85%         │
  └──────────────────────┴───────────────┴───────────────┘

  Why DeepEP is faster:
    1. GPU-initiated RDMA avoids CPU round-trips
    2. Token packing is fused into the communication kernel
    3. FP8 reduces data volume by 2x
    4. Better overlap with expert computation
    5. Optimized for IRREGULAR all-to-all (not just uniform)

  For inference (decode, low-latency kernel):
    NCCL all-to-all: ~200 μs per layer
    DeepEP low-lat:  ~80 μs per layer
    2.5x improvement → directly reduces inter-token latency.
```

---

## 10. API Usage

```python
import deepep

# Initialize communicator (one per expert-parallel group)
comm = deepep.Communicator(
    rank=rank,
    world_size=world_size,
    num_experts=256,
    use_low_latency=False,  # True for inference decode
)

# --- Forward pass ---

# Gate produces routing decisions
expert_ids, gate_scores = router(hidden_states)
# expert_ids: [batch_size, top_k] — which experts each token goes to
# gate_scores: [batch_size, top_k] — gating weights

# Dispatch: send tokens to their assigned expert's GPU
dispatched_tokens, recv_metadata = comm.dispatch(
    hidden_states,    # [num_tokens, hidden_dim]
    expert_ids,       # [num_tokens, top_k]
    gate_scores,      # [num_tokens, top_k]
    use_fp8=True,     # quantize for communication
)
# dispatched_tokens: tokens received by THIS GPU's expert
# recv_metadata: bookkeeping for the combine step

# Run local expert
expert_output = local_expert(dispatched_tokens)

# Combine: send expert outputs back to originating GPUs
combined_output = comm.combine(
    expert_output,
    recv_metadata,    # tells DeepEP where to send results back
)
# combined_output: [num_tokens, hidden_dim] — back on original GPUs
```

---

## 11. Comparison with NCCL All-to-All

```
┌─────────────────────────┬───────────────────┬──────────────────────┐
│  Feature                │  NCCL All-to-All  │  DeepEP              │
├─────────────────────────┼───────────────────┼──────────────────────┤
│  Designed for           │  General purpose  │  MoE specifically    │
│  Irregular sizes        │  Poor handling    │  Native support      │
│  GPU-initiated RDMA     │  No (CPU-driven)  │  Yes                 │
│  FP8 quantized comm     │  No               │  Built-in            │
│  Compute overlap        │  Manual           │  Automatic           │
│  Low-latency mode       │  No               │  Yes (for decode)    │
│  Dynamic buffer mgmt    │  Fixed buffers    │  Adaptive            │
│  Token packing          │  Separate step    │  Fused in kernel     │
│  Expert-aware routing   │  No               │  Yes                 │
│  Dependencies           │  Just CUDA        │  ibverbs, GDRCopy    │
│  Generality             │  Any collective   │  MoE dispatch/combine│
└─────────────────────────┴───────────────────┴──────────────────────┘

When to use NCCL:
  - Dense models (no MoE)
  - AllReduce, AllGather, ReduceScatter
  - When you need maximum portability

When to use DeepEP:
  - MoE models with expert parallelism
  - When dispatch/combine latency is a bottleneck
  - DeepSeek-style architectures
```

---

## 12. Key Takeaways

```
1. DeepEP is NOT a general-purpose replacement for NCCL.
   It solves ONE specific problem: MoE all-to-all communication.

2. Two kernels for two regimes:
   Normal (high throughput) for training/prefill.
   Low-latency for inference decode.

3. GPU-initiated RDMA eliminates CPU from the data path.
   Biggest latency win for small message sizes (decode).

4. FP8 communication halves bandwidth requirements.
   Critical when InfiniBand is the bottleneck (inter-node).

5. Overlap is first-class: dispatch/combine are designed
   to run concurrently with expert computation.

6. Open source: https://github.com/deepseek-ai/DeepEP
   Licensed under MIT. Requires InfiniBand hardware.

7. Adopted in DeepSeek-V3 training (671B parameters, 256 experts).
   Core enabler of their MoE scaling approach.
```
