# How Frontier Labs Train SOTA Models (2024-2026)

---

## 1. The Full Training Stack

```
Every large model training run has these layers:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  YOUR TRAINING CODE                                             │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  model = TransformerModel(...)                             │ │
  │  │  for batch in dataloader:                                  │ │
  │  │      loss = model(batch)                                   │ │
  │  │      loss.backward()                                       │ │
  │  │      optimizer.step()                                      │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │       │                                                         │
  │       ▼                                                         │
  │  FRAMEWORK (math + autograd)                                   │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  PyTorch (90% of labs)  or  JAX (Google, Anthropic)       │ │
  │  │  - Tensor operations                                      │ │
  │  │  - Automatic differentiation (backward pass)              │ │
  │  │  - GPU kernel dispatch (cuDNN, cuBLAS)                    │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │       │                                                         │
  │       ▼                                                         │
  │  PARALLELISM STRATEGY (distribute the model + data)            │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  Megatron-LM:  tensor + pipeline parallelism              │ │
  │  │  DeepSpeed:    ZeRO optimizer sharding + offloading       │ │
  │  │  FSDP:         PyTorch-native fully sharded data parallel │ │
  │  │  Megatron-DeepSpeed: combination (used by many labs)      │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │       │                                                         │
  │       ▼                                                         │
  │  COMMUNICATION (GPU-to-GPU data transfer)                      │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  NCCL:     NVIDIA Collective Communications Library       │ │
  │  │  - AllReduce (average gradients)                          │ │
  │  │  - AllGather (collect sharded parameters)                 │ │
  │  │  - ReduceScatter (shard the results)                      │ │
  │  │  Uses: NVLink (intra-node), InfiniBand/RoCE (inter-node) │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │       │                                                         │
  │       ▼                                                         │
  │  CLUSTER MANAGEMENT (launch jobs, allocate GPUs)               │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  Slurm:      HPC scheduler (Meta, NVIDIA, universities)  │ │
  │  │  Kubernetes:  container orchestration (cloud companies)    │ │
  │  │  Ray:         Python-native (startups, mid-scale)         │ │
  │  │  Borg:        Google internal (not available to anyone)   │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │       │                                                         │
  │       ▼                                                         │
  │  HARDWARE                                                      │
  │  ┌────────────────────────────────────────────────────────────┐ │
  │  │  GPU nodes:   NVIDIA DGX H100 (8× H100 per node)         │ │
  │  │  Networking:  InfiniBand HDR/NDR (400-800 Gbps)           │ │
  │  │              or RoCE (RDMA over Converged Ethernet)       │ │
  │  │  Storage:     Lustre / GPFS / NFS (shared filesystem)     │ │
  │  │  TPU pods:    Google TPU v4/v5p (alternative to GPUs)     │ │
  │  └────────────────────────────────────────────────────────────┘ │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘
```

---

## 2. Who Uses What

```
┌─────────────┬────────────┬────────────┬──────────┬──────────┬──────────────┐
│ Company     │ Framework  │ Parallelism│ Comms    │ Scheduler│ Hardware     │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ Meta        │ PyTorch    │ FSDP +     │ NCCL     │ Slurm   │ 16K H100     │
│ (LLaMA-3   │            │ custom     │          │          │ Grand Teton  │
│  405B)      │            │ TP+PP      │          │          │ RoCE network │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ Google      │ JAX        │ GSPMD      │ XLA      │ Borg     │ TPU v4/v5p   │
│ (Gemini)    │            │ (auto      │ collec-  │ (intern.)│ pods, ICI    │
│             │            │ partition) │ tives    │          │ network      │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ OpenAI      │ PyTorch    │ Megatron   │ NCCL     │ K8s on   │ 25K+ H100   │
│ (GPT-4+)    │            │ fork       │          │ Azure    │ InfiniBand   │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ Anthropic   │ JAX        │ Custom     │ XLA +    │ Ray +    │ TPUs +       │
│ (Claude)    │            │            │ NCCL     │ custom   │ some GPUs    │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ xAI         │ PyTorch    │ Custom     │ NCCL     │ Custom   │ 100K H100    │
│ (Grok)      │            │            │          │          │ Memphis DC   │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ DeepSeek    │ PyTorch    │ Megatron + │ NCCL     │ Slurm   │ ~10K H800    │
│ (V3, R1)    │            │ custom MoE │          │          │ (export      │
│             │            │ parallelism│          │          │  restricted) │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ NVIDIA      │ PyTorch    │ Megatron-LM│ NCCL     │ Slurm   │ DGX SuperPOD │
│ (Nemotron)  │            │            │          │          │ InfiniBand   │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ Microsoft   │ PyTorch    │ DeepSpeed  │ NCCL     │ K8s on   │ Azure GPU    │
│ (Phi)       │            │ (ZeRO)     │          │ Azure    │ clusters     │
├─────────────┼────────────┼────────────┼──────────┼──────────┼──────────────┤
│ Mistral     │ PyTorch    │ Megatron + │ NCCL     │ Slurm   │ H100 clusters│
│             │            │ custom     │          │          │ InfiniBand   │
└─────────────┴────────────┴────────────┴──────────┴──────────┴──────────────┘
```

---

## 3. The Three Types of Parallelism (3D Parallelism)

```
A 405B parameter model does NOT fit on one GPU.
  H100 has 80 GB memory.
  405B params in FP16 = 810 GB (just weights).
  + optimizer states (Adam) = ~3.2 TB.
  + activations + gradients = even more.

  You MUST split the model across many GPUs.
  The question is HOW.

THREE STRATEGIES (usually combined):

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  1. DATA PARALLELISM (DP)                                       │
  │     ─────────────────────                                       │
  │     Each GPU has a COPY of the full model.                      │
  │     Different GPUs process different batches.                   │
  │     After backward pass: AllReduce to average gradients.        │
  │                                                                  │
  │     GPU 0: model copy + batch 0 → gradients 0 ─┐               │
  │     GPU 1: model copy + batch 1 → gradients 1  ├─ AllReduce    │
  │     GPU 2: model copy + batch 2 → gradients 2  │  (average)    │
  │     GPU 3: model copy + batch 3 → gradients 3 ─┘               │
  │                                                                  │
  │     Pro:  simple, scales well                                   │
  │     Con:  each GPU needs full model → can't train > 80GB model │
  │                                                                  │
  │  2. TENSOR PARALLELISM (TP) — see megatron/                    │
  │     ──────────────────────                                      │
  │     Split INDIVIDUAL LAYERS across GPUs.                        │
  │     Each GPU holds part of the weight matrix.                   │
  │                                                                  │
  │     Layer with weight matrix [4096 × 16384]:                    │
  │       GPU 0: columns [0:4096]                                   │
  │       GPU 1: columns [4096:8192]                                │
  │       GPU 2: columns [8192:12288]                               │
  │       GPU 3: columns [12288:16384]                              │
  │                                                                  │
  │     Each GPU computes its chunk → AllReduce to combine.         │
  │                                                                  │
  │     Pro:  model layers can be arbitrarily large                 │
  │     Con:  VERY communication-heavy (AllReduce every layer)      │
  │           Must be within NVLink distance (same node)            │
  │                                                                  │
  │  3. PIPELINE PARALLELISM (PP)                                   │
  │     ─────────────────────────                                   │
  │     Split the model into STAGES (groups of layers).             │
  │     Each GPU holds different layers.                            │
  │                                                                  │
  │     GPU 0: layers 0-19   (stage 1)                              │
  │     GPU 1: layers 20-39  (stage 2)                              │
  │     GPU 2: layers 40-59  (stage 3)                              │
  │     GPU 3: layers 60-79  (stage 4)                              │
  │                                                                  │
  │     Data flows: GPU0 → GPU1 → GPU2 → GPU3 (forward)            │
  │                 GPU3 → GPU2 → GPU1 → GPU0 (backward)           │
  │                                                                  │
  │     Pro:  less communication than TP (only between stages)      │
  │     Con:  "pipeline bubble" — GPUs sit idle waiting for data    │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

3D PARALLELISM = all three combined:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  Example: training 405B on 1024 GPUs (128 nodes × 8 GPUs)      │
  │                                                                  │
  │  Tensor Parallel = 8  (within one node, over NVLink)            │
  │  Pipeline Parallel = 16 (across nodes, 16 stages)               │
  │  Data Parallel = 8   (8 identical pipeline replicas)            │
  │                                                                  │
  │  8 × 16 × 8 = 1024 GPUs ✓                                     │
  │                                                                  │
  │  ┌─────────── Data Parallel Replica 0 ──────────┐              │
  │  │                                               │              │
  │  │  Node 0 (8 GPUs, TP=8)  ← Pipeline Stage 0  │              │
  │  │  Node 1 (8 GPUs, TP=8)  ← Pipeline Stage 1  │              │
  │  │  ...                                          │              │
  │  │  Node 15 (8 GPUs, TP=8) ← Pipeline Stage 15 │              │
  │  │                                               │              │
  │  └───────────────────────────────────────────────┘              │
  │                                                                  │
  │  ┌─────────── Data Parallel Replica 1 ──────────┐              │
  │  │  (same structure, different data batches)      │              │
  │  └───────────────────────────────────────────────┘              │
  │  ...                                                            │
  │  ┌─────────── Data Parallel Replica 7 ──────────┐              │
  │  │                                               │              │
  │  └───────────────────────────────────────────────┘              │
  │                                                                  │
  │  Communication patterns:                                        │
  │    TP (within node):   AllReduce over NVLink (900 GB/s)        │
  │    PP (between nodes): point-to-point over InfiniBand (400Gbps)│
  │    DP (all replicas):  AllReduce over InfiniBand               │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

WHY this specific split:
  TP within a node:  NVLink has 900 GB/s bandwidth.
    TP needs AllReduce at EVERY layer — must be fast.
  PP across nodes:   only sends activations between stages.
    Lower bandwidth OK — InfiniBand's 400 Gbps is enough.
  DP across replicas: AllReduce gradients once per step.
    Can overlap with backward pass computation.
```

---

## 4. The Hardware — What a Training Cluster Looks Like

```
NVIDIA DGX H100 NODE (the standard unit):

  ┌──────────────────────────────────────────────────────────────┐
  │  DGX H100 (one node)                                        │
  │                                                              │
  │  8× H100 SXM GPUs                                          │
  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐                          │
  │  │H100 │ │H100 │ │H100 │ │H100 │                          │
  │  │80GB │ │80GB │ │80GB │ │80GB │                          │
  │  └──┬──┘ └──┬──┘ └──┬──┘ └──┬──┘                          │
  │     │       │       │       │                               │
  │     ╰───────┴───────┴───────╯                               │
  │          NVSwitch (all-to-all NVLink)                       │
  │          900 GB/s bidirectional between any pair             │
  │     ╭───────┬───────┬───────╮                               │
  │  ┌──┴──┐ ┌──┴──┐ ┌──┴──┐ ┌──┴──┐                          │
  │  │H100 │ │H100 │ │H100 │ │H100 │                          │
  │  │80GB │ │80GB │ │80GB │ │80GB │                          │
  │  └─────┘ └─────┘ └─────┘ └─────┘                          │
  │                                                              │
  │  2× CPU (Intel Xeon or AMD EPYC)                            │
  │  2 TB system RAM                                             │
  │  8× ConnectX-7 NICs (each 400 Gbps InfiniBand)             │
  │  30 TB NVMe SSD                                             │
  │                                                              │
  │  Total GPU memory: 640 GB per node                          │
  │  Total GPU compute: ~8 × 990 TFLOPS FP16 ≈ 8 PFLOPS       │
  │  Network to outside: 3.2 Tbps (8 × 400 Gbps)              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

SUPERPOD (256 DGX H100 nodes):

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  256 DGX H100 nodes = 2,048 H100 GPUs                      │
  │                                                              │
  │  Connected by:                                              │
  │    InfiniBand NDR switches (leaf-spine topology)            │
  │    NVIDIA Quantum-2 switches                                │
  │    Full bisection bandwidth                                 │
  │    Any GPU can talk to any other GPU at full speed           │
  │                                                              │
  │  ┌─────┐  ┌─────┐  ┌─────┐        ┌─────┐                 │
  │  │Node0│  │Node1│  │Node2│  ...   │N255 │                 │
  │  └──┬──┘  └──┬──┘  └──┬──┘        └──┬──┘                 │
  │     │        │        │              │                      │
  │  ┌──┴────────┴────────┴──────────────┴───┐                 │
  │  │     InfiniBand Leaf Switches          │                 │
  │  └──┬────────┬────────┬──────────────┬───┘                 │
  │     │        │        │              │                      │
  │  ┌──┴────────┴────────┴──────────────┴───┐                 │
  │  │     InfiniBand Spine Switches         │                 │
  │  └──────────────────────────────────────┘                  │
  │                                                              │
  │  Shared storage:                                            │
  │    Lustre or GPFS parallel filesystem                      │
  │    Petabytes of capacity, 100+ GB/s aggregate read         │
  │    All nodes see the same filesystem                       │
  │    Checkpoints, datasets, code all stored here              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

Meta's LLaMA-3 cluster:
  2× SuperPODs = ~16,384 H100 GPUs
  Used RoCE (RDMA over Ethernet) instead of InfiniBand
  Custom Grand Teton OCP server design (not DGX)

xAI's Memphis cluster:
  100,000 H100 GPUs (largest known cluster, 2024)
  Built in ~6 months
```

---

## 5. The Training Run — What Actually Happens

```
Training LLaMA-3-405B (Meta's public description):

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  PREPARATION (weeks before training)                            │
  │                                                                  │
  │  1. Data pipeline                                               │
  │     - 15 trillion tokens of text data                           │
  │     - Deduplicated, filtered, tokenized                         │
  │     - Stored on shared filesystem (Lustre)                      │
  │     - Pre-sharded: each GPU will read its own shard             │
  │                                                                  │
  │  2. Model architecture                                          │
  │     - 405B parameters, 126 transformer layers                   │
  │     - Hidden dim: 16384, 128 attention heads                    │
  │     - GQA (Grouped Query Attention) with 8 KV heads             │
  │                                                                  │
  │  3. Parallelism config                                          │
  │     - TP = 8 (within each node)                                 │
  │     - PP = 16 (16 pipeline stages across nodes)                 │
  │     - DP = 128 (128 data-parallel replicas)                     │
  │     - Total: 8 × 16 × 128 = 16,384 GPUs                       │
  │     - Context parallel: 2 (split long sequences)                │
  │                                                                  │
  │  TRAINING (runs for weeks)                                      │
  │                                                                  │
  │  4. Launch via Slurm                                            │
  │     sbatch train_405b.sh                                        │
  │     - Allocates 2048 nodes (16384 GPUs)                         │
  │     - Starts one process per GPU (16384 processes)              │
  │     - Each process: python train.py --rank=N --world-size=16384 │
  │                                                                  │
  │  5. Each training step                                          │
  │     a) Data loading:                                             │
  │        Each DP replica reads its batch from Lustre              │
  │        ~4M tokens per step (global batch size)                  │
  │        Each GPU processes ~250 tokens per microbatch            │
  │                                                                  │
  │     b) Forward pass:                                             │
  │        Microbatch flows through pipeline stages (PP)            │
  │        Within each stage, layers computed across TP group       │
  │        Multiple microbatches in-flight (1F1B schedule)          │
  │                                                                  │
  │     c) Backward pass:                                            │
  │        Gradients flow backward through pipeline                 │
  │        TP groups AllReduce within each layer                    │
  │                                                                  │
  │     d) Gradient sync:                                            │
  │        AllReduce gradients across DP replicas                   │
  │        This runs OVERLAP with backward pass (async AllReduce)   │
  │                                                                  │
  │     e) Optimizer step:                                           │
  │        Adam update on each GPU's shard of parameters            │
  │                                                                  │
  │     f) Log metrics:                                              │
  │        Loss, learning rate, throughput, GPU utilization          │
  │                                                                  │
  │  6. Checkpointing (every N steps, e.g., every 1000)            │
  │     - Each GPU saves its shard of model + optimizer to Lustre   │
  │     - Async: checkpoint writing overlaps with next train step   │
  │     - Total checkpoint size: ~8 TB (model + optimizer + state)  │
  │     - ~10 minutes to write at aggregate bandwidth               │
  │                                                                  │
  │  7. Failure handling                                            │
  │     - At 16K GPU scale: ~1-2 GPU failures PER DAY              │
  │     - On failure: all processes stop                            │
  │     - Slurm restarts the job                                    │
  │     - Resume from last checkpoint                               │
  │     - Lost work: at most 1000 steps (~30-60 minutes)            │
  │     - Meta reported ~2-3% time lost to failures for LLaMA-3    │
  │                                                                  │
  │  TOTAL TRAINING TIME                                            │
  │  ~54 days, ~3.8 × 10²⁵ FLOPs                                  │
  │  Estimated cost: $30-60M at cloud GPU rates                     │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘
```

---

## 6. Networking — Why It's the Bottleneck

```
Training is compute-bound locally but NETWORK-BOUND globally.

  Within a node (NVLink):
    900 GB/s bidirectional between any pair of GPUs.
    H100 compute: ~990 TFLOPS FP16.
    Compute-to-bandwidth ratio: 990 / 900 ≈ 1.1 FLOPS/byte.
    → NVLink can keep up with compute. Not the bottleneck.

  Between nodes (InfiniBand):
    400 Gbps = 50 GB/s per link. 8 links = 400 GB/s per node.
    But shared across 8 GPUs: effectively ~50 GB/s per GPU.
    Compute-to-bandwidth ratio: 990 / 50 ≈ 20 FLOPS/byte.
    → Network is 20× slower than compute. THIS is the bottleneck.

  This is why the parallelism strategy matters:
    TP (needs AllReduce every layer) → MUST be within NVLink.
    PP (sends activations between stages) → can tolerate InfiniBand.
    DP (AllReduce once per step) → can overlap with computation.

  ┌──────────────────────────────────────────────────────────────┐
  │ Interconnect    │ Bandwidth │ Latency │ Used for             │
  ├─────────────────┼───────────┼─────────┼──────────────────────┤
  │ NVLink (H100)   │ 900 GB/s  │ ~1 μs   │ TP within node      │
  │ InfiniBand NDR  │ 400 Gbps  │ ~1 μs   │ PP, DP across nodes │
  │ RoCE v2         │ 400 Gbps  │ ~2-5 μs │ Meta's alternative  │
  │ Ethernet (TCP)  │ 100 Gbps  │ ~50 μs  │ DON'T use for train │
  │ TPU ICI         │ ~4.8 Tbps │ ~5 μs   │ Google TPU pods     │
  └─────────────────┴───────────┴─────────┴──────────────────────┘

  Why InfiniBand and not regular Ethernet?
    1. RDMA (Remote Direct Memory Access):
       GPU → NIC → network → NIC → GPU. No CPU involvement.
       Regular Ethernet: GPU → CPU → kernel → NIC → ... → CPU → GPU.
       RDMA skips the CPU and kernel entirely.

    2. Lossless fabric:
       InfiniBand guarantees no packet drops (credit-based flow control).
       Regular Ethernet drops packets under congestion → retransmits → spikes.
       NCCL AllReduce can't tolerate latency variance.

    3. Predictable latency:
       ~1 μs, every time. Ethernet can spike to milliseconds.
```

---

## 7. Mixed Precision — How It's Done

```
Nobody trains in FP32 anymore. It wastes memory and compute.

  FP32 (full precision):    32 bits per parameter. Slow, large.
  FP16 (half precision):    16 bits. 2× faster, 2× less memory.
  BF16 (brain float 16):   16 bits, larger range than FP16.
  FP8 (H100 feature):      8 bits. 4× faster than FP32.

  Standard practice (BF16 mixed precision):
    Weights:               stored in BF16 (16 bits)
    Activations:           computed in BF16
    Gradients:             computed in BF16
    Optimizer states:      stored in FP32 (Adam needs precision)
    Master weights:        stored in FP32 (for accumulation)

  Memory per parameter:
    BF16 weight:           2 bytes
    FP32 master weight:    4 bytes  (for accurate accumulation)
    FP32 Adam momentum:    4 bytes
    FP32 Adam variance:    4 bytes
    Gradient (BF16):       2 bytes
    ──────────────────────────────
    Total:                 16 bytes per parameter

    405B params × 16 bytes = 6.5 TB
    ÷ 16,384 GPUs = 400 MB per GPU (just model state)
    + activations: depends on batch size and sequence length

  Why BF16 over FP16:
    BF16 has same exponent range as FP32 (8 bits).
    FP16 has only 5 exponent bits → overflows with large gradients.
    BF16 never overflows → no loss scaling needed. Simpler.
    H100/A100 have native BF16 tensor cores — same speed as FP16.
```

---

## 8. Checkpointing — Saving Progress

```
At 16K GPU scale, hardware failures happen EVERY DAY.
Without checkpointing, a failure means restarting from scratch.

  Checkpoint = snapshot of:
    - Model weights (all shards)
    - Optimizer states (momentum, variance for each param)
    - Learning rate scheduler state
    - Data loader position (which samples have been seen)
    - RNG states (for reproducibility)

  Total size: ~4-8 TB for a 405B model

  The challenge: writing 8 TB to disk while 16K GPUs wait.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  SYNCHRONOUS CHECKPOINT (simple but slow):                  │
  │    1. All GPUs stop training                                │
  │    2. Each GPU writes its shard to Lustre                   │
  │    3. Wait for all writes to complete                       │
  │    4. Resume training                                       │
  │    Time: 5-15 minutes. GPUs idle the entire time.           │
  │                                                              │
  │  ASYNC CHECKPOINT (what labs actually use):                 │
  │    1. Copy model state to CPU memory (fast, ~seconds)       │
  │    2. Resume training immediately on GPU                    │
  │    3. Background thread writes CPU copy to Lustre           │
  │    4. GPU never stops training                              │
  │    Time lost: ~10-30 seconds (just the CPU copy).           │
  │                                                              │
  │  Even smarter: in-memory checkpointing                     │
  │    Keep last checkpoint in CPU RAM across all nodes.        │
  │    On failure: neighboring node has the checkpoint in RAM.  │
  │    No disk read needed for recovery. ~30 second restart.    │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Checkpoint frequency tradeoff:
    Too frequent:  wastes time writing checkpoints
    Too infrequent: wastes time re-computing on failure

    Optimal: depends on failure rate.
    Meta (LLaMA-3): every ~1000 steps (~30-60 min of training)
    At 1-2 failures/day: lose ~30 min per failure.
    Total wasted time: ~2-3% of wall clock.
```

---

## 9. Failure Handling at Scale

```
  MTBF (Mean Time Between Failures) at scale:

    1 GPU MTBF:      ~10,000 hours (very reliable)
    16,384 GPUs:     16384 × (1/10000) = ~1.6 failures/hour

    But not all failures are equal:
      - GPU memory error (ECC):    ~1/day. Kill that process.
      - NIC flap:                  ~1/day. Retry communication.
      - Node crash:                ~1/week. Restart node.
      - Switch failure:            ~1/month. Reroute traffic.
      - Power event:               ~1/quarter. Multiple nodes down.

  Meta's LLaMA-3 paper reported:
    419 interruptions over 54 days of training.
    = 7.8 interruptions per day.
    Only 78 were unexpected (hardware/software crashes).
    The rest were planned maintenance or proactive restarts.

  How failures are handled:

    1. NCCL operation hangs (GPU timeout, 5-30 min default)
    2. Watchdog detects the hang
    3. All processes are killed (SIGTERM to all ranks)
    4. Slurm health checks the nodes:
       - Run GPU diagnostics (nvidia-smi, dcgmi)
       - Mark bad node as "drained" (excluded from scheduling)
       - Replace with spare node from warm pool
    5. Slurm restarts the full job on healthy nodes
    6. All processes load last checkpoint from Lustre
    7. Training resumes

  Time from failure to resume: 5-30 minutes
    Detection: 30s - 5min (depends on NCCL timeout)
    Diagnostics: 1-2 min
    Node replacement: 1-2 min (from warm pool)
    Job startup: 2-5 min (16K processes initializing)
    Checkpoint load: 2-5 min (8 TB from Lustre)
```

---

## 10. Data Loading at Scale

```
  Problem: 16,384 GPUs all need continuous data.
  If each GPU reads from Lustre independently → filesystem dies.

  Solution: distributed data loading pipeline.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Data preparation (OFFLINE, before training):               │
  │                                                              │
  │  1. Raw text → tokenize → pack into fixed-length sequences │
  │  2. Store as memory-mapped binary files                     │
  │     (e.g., .bin files, numpy memmap format)                 │
  │  3. Pre-shard into N files (N = number of DP replicas)     │
  │     Each DP replica reads its own shard file.               │
  │     No cross-GPU coordination during training.              │
  │                                                              │
  │  During training:                                           │
  │                                                              │
  │  Each worker:                                               │
  │    - mmap() its shard file (Lustre handles caching)        │
  │    - Sequential read (small random offset for shuffling)    │
  │    - Prefetch next batches while GPU is computing           │
  │    - Data → CPU RAM → GPU (via PCIe or pinned memory)      │
  │                                                              │
  │  Key tricks:                                                │
  │    - Pre-tokenized: no CPU processing during training       │
  │    - Pre-sharded: no coordination between workers           │
  │    - Memory-mapped: OS handles caching, no manual IO        │
  │    - Prefetch: data transfer overlaps with GPU compute      │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

---

## 11. The Numbers

```
┌────────────────────────┬──────────────────────────────────────────┐
│                        │ LLaMA-3 405B (Meta, 2024)               │
├────────────────────────┼──────────────────────────────────────────┤
│ Parameters             │ 405 billion                              │
│ Training tokens        │ 15 trillion                              │
│ Total FLOPs            │ ~3.8 × 10²⁵                             │
│ GPUs                   │ 16,384 H100                              │
│ Training time          │ ~54 days                                 │
│ GPU utilization (MFU)  │ ~38-43%                                  │
│ Throughput             │ ~16M tokens/sec                          │
│ Global batch size      │ ~16M tokens                              │
│ Sequence length        │ 8192 tokens                              │
│ Learning rate peak     │ 8 × 10⁻⁵                                │
│ Checkpoint size        │ ~8 TB                                    │
│ Checkpoint frequency   │ every ~1000 steps                        │
│ Failures (54 days)     │ 419 interruptions (78 unexpected)        │
│ Time lost to failures  │ ~2-3%                                    │
│ Estimated cost         │ $30-60M (cloud equivalent)               │
│ Networking             │ RoCE v2, 400 Gbps per NIC, 8 NICs/node  │
│ Storage                │ Lustre parallel filesystem               │
│ Parallelism            │ TP=8, PP=16, DP=128, CP=2               │
├────────────────────────┼──────────────────────────────────────────┤
│                        │ GPT-4 (estimated)                        │
├────────────────────────┼──────────────────────────────────────────┤
│ Parameters             │ ~1.8 trillion (MoE, estimated)           │
│ GPUs                   │ ~25,000 A100                             │
│ Training time          │ ~90 days (estimated)                     │
│ Estimated cost         │ ~$100M                                   │
│ Networking             │ InfiniBand                               │
│ Cluster                │ Azure supercomputer                      │
├────────────────────────┼──────────────────────────────────────────┤
│                        │ Gemini Ultra (estimated)                 │
├────────────────────────┼──────────────────────────────────────────┤
│ Parameters             │ ~1 trillion+ (estimated)                 │
│ Hardware               │ TPU v4 pods (thousands of chips)        │
│ Training time          │ months (estimated)                       │
│ Framework              │ JAX + XLA                                │
│ Scheduling             │ Borg (Google internal)                   │
└────────────────────────┴──────────────────────────────────────────┘

GPU Utilization (MFU = Model FLOPS Utilization):
  Theoretical peak:  990 TFLOPS per H100 (BF16)
  Actual achieved:   ~380-425 TFLOPS per H100 (~38-43% MFU)

  Where the rest goes:
    - Communication overhead (AllReduce, pipeline bubbles)
    - Memory-bound operations (attention, layernorm)
    - Data loading latency
    - Checkpoint overhead
    - Python/framework overhead

  40% MFU is GOOD at this scale. 50%+ is excellent.
```

---

## 12. Learning Path — What to Study Next

```
Read the deep dives in order:

  1. nccl/README.md
     How GPU-to-GPU communication works.
     AllReduce, ring algorithm, NVLink vs InfiniBand.
     This is the foundation — everything depends on NCCL.

  2. megatron/README.md
     Tensor parallelism and pipeline parallelism.
     How to split a transformer across GPUs.
     The 1F1B pipeline schedule.

  3. deepspeed/README.md
     ZeRO optimizer sharding (stages 1, 2, 3).
     Offloading to CPU/NVMe.
     When to use DeepSpeed vs Megatron vs FSDP.

  4. slurm/README.md
     HPC job scheduling.
     How sbatch/srun work. Node allocation.
     How failures are detected and jobs restarted.

  Related topics:
    - jax/       → Google's alternative to PyTorch
    - xla/       → compiler for JAX/TensorFlow (TPU + GPU)
    - triton/    → write GPU kernels in Python (Flash Attention uses this)
    - arrow/     → columnar memory format (used by Ray, Spark, pandas)
```
