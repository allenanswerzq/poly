# HybridDEP — Hybrid Dispatch for Expert Parallelism

---

## 1. What HybridDEP Does

```
HybridDEP (Hybrid Dispatch for Expert Parallelism) extends DeepEP
to handle the PREFILL-DECODE disaggregation problem in MoE inference.

The key insight:

  Modern MoE inference systems separate prefill and decode:
    - PREFILL nodes: process the full prompt (compute-heavy)
    - DECODE nodes: generate tokens one at a time (memory-bound)

  But MoE expert parallelism creates a problem:

    Prefill nodes have MANY tokens → large all-to-all (bandwidth-bound)
    Decode nodes have FEW tokens  → small all-to-all (latency-bound)

  DeepEP already has two kernels for this.
  But what happens when prefill and decode nodes SHARE the same
  expert-parallel group? Or when the system transitions between phases?

  HybridDEP solves this by providing a UNIFIED dispatch mechanism
  that dynamically switches between high-throughput and low-latency
  strategies within the same communication group.

  ┌─────────────────────────────────────────────────────────────┐
  │  DeepEP:    Two separate kernels, pick one at init time.   │
  │  HybridDEP: One unified system, adapts per-operation.      │
  └─────────────────────────────────────────────────────────────┘
```

---

## 2. The Disaggregated MoE Problem

```
In a disaggregated MoE serving system (like DeepSeek's production setup):

  ┌─────────────────────────────────────────────────────────────────┐
  │                     EXPERT PARALLEL GROUP                       │
  │                                                                 │
  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
  │  │ Node 0   │  │ Node 1   │  │ Node 2   │  │ Node 3   │      │
  │  │ Expert   │  │ Expert   │  │ Expert   │  │ Expert   │      │
  │  │ 0-63     │  │ 64-127   │  │ 128-191  │  │ 192-255  │      │
  │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘      │
  │       │              │              │              │            │
  │       └──────────────┴──────┬───────┴──────────────┘            │
  │                             │                                   │
  │                    InfiniBand fabric                            │
  │                             │                                   │
  │       ┌──────────────┬──────┴───────┬──────────────┐           │
  │       │              │              │              │            │
  │  ┌────┴─────┐  ┌────┴─────┐  ┌────┴─────┐  ┌────┴─────┐      │
  │  │ Prefill  │  │ Prefill  │  │ Decode   │  │ Decode   │      │
  │  │ Node A   │  │ Node B   │  │ Node C   │  │ Node D   │      │
  │  │ (many    │  │ (many    │  │ (few     │  │ (few     │      │
  │  │  tokens) │  │  tokens) │  │  tokens) │  │  tokens) │      │
  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘      │
  └─────────────────────────────────────────────────────────────────┘

  Problem: Expert nodes receive traffic from BOTH prefill and decode nodes.

  Prefill Node A sends 4096 tokens → Expert nodes (large dispatch)
  Decode Node C sends 32 tokens    → Expert nodes (tiny dispatch)

  These arrive at the SAME expert nodes, potentially at the same time.

  If expert nodes use the normal (throughput) kernel:
    Decode traffic suffers high latency (batched with prefill traffic).
    Decode users experience slow generation.

  If expert nodes use the low-latency kernel:
    Prefill traffic gets poor throughput (suboptimal for large batches).
    Time-to-first-token increases.

  HybridDEP: expert nodes handle BOTH traffic types efficiently,
  using the right strategy for each incoming dispatch.
```

---

## 3. Hybrid Dispatch Strategy

```
HybridDEP classifies each dispatch operation and routes it
through the appropriate code path:

  ┌─────────────────────────────────────────────────────────────┐
  │                    INCOMING DISPATCH                        │
  │                         │                                   │
  │                    ┌────▼────┐                              │
  │                    │ CLASSIFY │                             │
  │                    │ by token │                             │
  │                    │  count   │                             │
  │                    └────┬────┘                              │
  │                         │                                   │
  │              ┌──────────┼──────────┐                       │
  │              │          │          │                        │
  │         ┌────▼────┐ ┌───▼────┐ ┌──▼──────┐               │
  │         │  LARGE  │ │ MEDIUM │ │  SMALL  │               │
  │         │ >1024   │ │ 64-1024│ │  <64    │               │
  │         │ tokens  │ │ tokens │ │ tokens  │               │
  │         └────┬────┘ └───┬────┘ └──┬──────┘               │
  │              │          │         │                        │
  │         ┌────▼────┐ ┌───▼────┐ ┌──▼──────┐               │
  │         │ NORMAL  │ │ HYBRID │ │LOW-LAT  │               │
  │         │ KERNEL  │ │  PATH  │ │ KERNEL  │               │
  │         │ (bulk   │ │ (adapt │ │ (eager  │               │
  │         │  RDMA)  │ │  ive)  │ │  send)  │               │
  │         └─────────┘ └────────┘ └─────────┘               │
  └─────────────────────────────────────────────────────────────┘

  LARGE dispatches (prefill):
    Use the normal kernel — maximize bandwidth utilization.
    Pack tokens into large RDMA transfers.
    Overlap with expert computation.

  SMALL dispatches (decode):
    Use the low-latency kernel — minimize dispatch time.
    Eager RDMA writes, no packing delay.
    Priority scheduling on expert nodes.

  MEDIUM dispatches (mixed / chunked prefill):
    Hybrid path — start with eager sends, switch to bulk
    if more tokens keep arriving. Adaptive threshold.
```

---

## 4. Priority Scheduling on Expert Nodes

```
When expert nodes serve both prefill and decode traffic,
decode tokens should get PRIORITY:

  Why? Decode latency directly impacts user-perceived generation speed.
  A 100μs delay per layer × 80 layers = 8ms per token.
  At 30 tokens/sec target: 8ms is 24% of the budget!

  Prefill can tolerate slightly higher latency (batched anyway).

  HybridDEP priority mechanism:

  ┌─────────────────────────────────────────────────────────────┐
  │  Expert Node Receive Queue                                  │
  │                                                             │
  │  ┌────────────────────────────────────────────────────────┐ │
  │  │  HIGH PRIORITY (decode tokens)                         │ │
  │  │  [D1] [D2] [D3]                          → process    │ │
  │  │                                             FIRST      │ │
  │  ├────────────────────────────────────────────────────────┤ │
  │  │  NORMAL PRIORITY (prefill tokens)                      │ │
  │  │  [P1 P2 P3 P4 P5 P6 P7 P8 P9 ...]     → process     │ │
  │  │                                           after decode │ │
  │  └────────────────────────────────────────────────────────┘ │
  │                                                             │
  │  Implementation:                                            │
  │  - Decode tokens are tagged with a priority flag in the    │
  │    RDMA header                                              │
  │  - Expert GPU kernel checks priority queue first           │
  │  - Decode tokens skip ahead of any pending prefill batch   │
  │  - Expert compute is preemptible at token boundaries       │
  │    (finish current token, check for high-priority work)    │
  └─────────────────────────────────────────────────────────────┘

  Result: decode tokens experience near-idle-system latency
  even when expert nodes are heavily loaded with prefill work.
```

---

## 5. Overlapping Prefill and Decode Communication

```
The hardest part: prefill and decode all-to-all operations
share the SAME InfiniBand links to expert nodes.

  Without HybridDEP (naive time-division):

  |==== prefill dispatch ====|== decode dispatch ==|== prefill combine ==|
  ↑                                                                      ↑
  decode waits for prefill    decode finally runs    decode waits again
  to finish its dispatch      but prefill is now
                              waiting for decode

  Terrible: decode latency = prefill dispatch time + decode time.

  With HybridDEP (interleaved):

  |== prefill dispatch (chunk 1) ==|
       |= decode dispatch =|        ← decode slips in between chunks
  |== prefill dispatch (chunk 2) ======|
                            |= decode combine =|
  |== prefill dispatch (chunk 3) ==============|
  |======= prefill combine ================|

  How it works:
  1. Prefill dispatch is CHUNKED into smaller pieces.
  2. Between chunks, decode traffic is injected.
  3. InfiniBand QoS (Quality of Service) gives decode
     traffic higher priority at the network level.
  4. Expert nodes process decode tokens during prefill gaps.

  Result:
    Prefill throughput: ~90% of dedicated (small overhead from chunking)
    Decode latency: ~110% of dedicated (small overhead from sharing)
    Overall: much better than time-division multiplexing.
```

---

## 6. Dynamic Expert Group Management

```
In production, the ratio of prefill to decode traffic changes
over time based on user request patterns:

  Morning: lots of new conversations → heavy prefill
  Afternoon: ongoing conversations → heavy decode
  Burst: viral prompt → sudden prefill spike

  HybridDEP adapts:

  ┌─────────────────────────────────────────────────────────────┐
  │  MONITORING                                                 │
  │  - Track dispatch sizes per source node                    │
  │  - Measure actual prefill/decode ratio                     │
  │  - Monitor expert node queue depths                        │
  │                                                             │
  │  ADAPTATION                                                 │
  │  - Adjust chunking granularity for prefill dispatches      │
  │    (more decode traffic → smaller prefill chunks)          │
  │  - Tune priority scheduling thresholds                     │
  │  - Rebalance expert placement across nodes if needed       │
  │                                                             │
  │  FEEDBACK LOOP                                              │
  │  - Decode latency target: <100 μs per dispatch            │
  │  - If decode latency > target: increase decode priority    │
  │  - If prefill throughput drops >15%: relax decode priority │
  │  - Control loop runs every ~100 dispatches                 │
  └─────────────────────────────────────────────────────────────┘
```

---

## 7. Memory Architecture

```
HybridDEP must manage memory for both prefill and decode buffers
simultaneously, unlike DeepEP which only handles one at a time.

  ┌─────────────────────────────────────────────────────────────┐
  │  GPU MEMORY LAYOUT (Expert Node)                           │
  │                                                             │
  │  ┌───────────────────────────────────────────────────────┐ │
  │  │  EXPERT WEIGHTS (read-only, pinned)                   │ │
  │  │  64 experts × params per expert                       │ │
  │  └───────────────────────────────────────────────────────┘ │
  │                                                             │
  │  ┌───────────────────────────────────────────────────────┐ │
  │  │  DECODE BUFFER POOL (pinned, pre-allocated)           │ │
  │  │  Small, fixed-size. Always available.                  │ │
  │  │  RDMA-registered for low-latency kernel access.       │ │
  │  │  Size: max_decode_batch × hidden_dim × dtype          │ │
  │  └───────────────────────────────────────────────────────┘ │
  │                                                             │
  │  ┌───────────────────────────────────────────────────────┐ │
  │  │  PREFILL BUFFER POOL (dynamic, elastic)               │ │
  │  │  Grows/shrinks based on prefill traffic.              │ │
  │  │  RDMA-registered on allocation.                       │ │
  │  │  Can be reclaimed if decode needs more headroom.      │ │
  │  └───────────────────────────────────────────────────────┘ │
  │                                                             │
  │  ┌───────────────────────────────────────────────────────┐ │
  │  │  WORKSPACE (expert computation scratch space)          │ │
  │  └───────────────────────────────────────────────────────┘ │
  └─────────────────────────────────────────────────────────────┘

  Key design decisions:
    1. Decode buffers are PINNED and never reclaimed
       → guarantees decode can always proceed without allocation
    2. Prefill buffers are elastic
       → can give memory back under memory pressure
    3. Both pools are RDMA-registered at allocation time
       → avoids expensive re-registration on each dispatch
```

---

## 8. Comparison: DeepEP vs HybridDEP

```
┌──────────────────────────┬───────────────────┬──────────────────────┐
│  Feature                 │  DeepEP           │  HybridDEP           │
├──────────────────────────┼───────────────────┼──────────────────────┤
│  Target workload         │  Training OR      │  Mixed prefill +     │
│                          │  inference (pick) │  decode (both)       │
│  Kernel selection        │  At init time     │  Per-operation       │
│  Prefill-decode coexist  │  No               │  Yes                 │
│  Priority scheduling     │  No               │  Decode priority     │
│  Chunked dispatch        │  No               │  Yes (for prefill)   │
│  Buffer management       │  Single pool      │  Dual pool           │
│  Adaptive thresholds     │  No               │  Yes (feedback loop) │
│  Disaggregated serving   │  Partial          │  Native support      │
│  Complexity              │  Lower            │  Higher              │
│  Use case                │  Single-phase     │  Production serving  │
│                          │  workloads        │  with mixed traffic  │
└──────────────────────────┴───────────────────┴──────────────────────┘

When to use DeepEP alone:
  - Pure training (no inference)
  - Inference with ONLY prefill or ONLY decode at a time
  - Simpler deployment

When to use HybridDEP:
  - Production inference with disaggregated prefill/decode
  - Expert nodes serving both prefill and decode simultaneously
  - When decode latency SLA is critical
  - DeepSeek-style deployment at scale
```

---

## 9. Integration with Serving Systems

```
HybridDEP fits into a disaggregated MoE serving architecture:

  ┌──────────────────────────────────────────────────────────────┐
  │  REQUEST ROUTER                                              │
  │  (assigns requests to prefill or decode nodes)               │
  │  │                                                           │
  │  ├──► Prefill Scheduler                                      │
  │  │    - Batches new prompts                                  │
  │  │    - Sends token batches via HybridDEP (normal mode)     │
  │  │    - Transfers KV cache to decode nodes after prefill     │
  │  │                                                           │
  │  └──► Decode Scheduler                                       │
  │       - Manages continuous batching                           │
  │       - Sends single-token batches via HybridDEP (low-lat)  │
  │       - Receives expert outputs with priority                │
  │                                                               │
  │  EXPERT NODES (shared by both)                               │
  │  - Run HybridDEP receiver                                    │
  │  - Process decode tokens first (priority queue)              │
  │  - Process prefill tokens in chunks                          │
  │  - Send results back via HybridDEP combine                  │
  └──────────────────────────────────────────────────────────────┘

  The serving system only needs to:
    1. Tag each dispatch as "prefill" or "decode"
    2. HybridDEP handles routing, priority, and overlap
    3. Expert nodes are managed transparently
```

---

## 10. Key Takeaways

```
1. HybridDEP extends DeepEP for DISAGGREGATED MoE inference.
   The core challenge: expert nodes serve both prefill and decode.

2. Per-operation kernel selection (not per-initialization).
   Each dispatch picks the right kernel based on token count.

3. Decode tokens get PRIORITY over prefill tokens.
   Critical for maintaining low generation latency under load.

4. Prefill dispatches are CHUNKED to create gaps for decode.
   Interleaving > time-division for mixed workloads.

5. Dual buffer pool: pinned (decode) + elastic (prefill).
   Decode always has memory; prefill adapts to availability.

6. Adaptive feedback loop tunes scheduling in real-time.
   Responds to changing prefill/decode traffic ratios.

7. Built on DeepEP's GPU-initiated RDMA and FP8 communication.
   Same performance foundations, extended for mixed workloads.

8. Designed for production MoE serving at DeepSeek scale.
   671B parameter model, 256 experts, thousands of concurrent users.
```
