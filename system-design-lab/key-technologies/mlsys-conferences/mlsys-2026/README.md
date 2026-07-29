# MLSys 2026 — Attendee Learning Guide

> Conference: **MLSys 2026**, Ninth Annual Conference on Machine Learning and Systems
> Location: **Bellevue, WA**
> Dates: **May 18-22, 2026**
> Site: https://mlsys.org/

---

## 1. Should You Go?

Yes — if you live close, this is one of the best conferences to attend for learning what is happening in modern AI infrastructure.

MLSys is especially relevant to this lab because it sits exactly at the intersection of:

```
ML models + distributed systems + compilers + GPUs/TPUs + serving + training + agents
```

It is not just theory. The program is full of production-oriented topics:

- LLM serving systems
- LLM training systems
- KV cache management
- RL for LLMs
- agentic AI systems
- GPU kernels and compilers
- model compression and quantization
- benchmarks and profiling
- datacenter scheduling
- distributed training reliability
- privacy and security for ML systems

If you can attend even one or two days, go.

---

## 2. Can You Attend Without a Paper or Invitation?

Yes. For a conference like MLSys, you generally do **not** need an accepted paper, poster, invitation, or university affiliation to attend as a normal attendee.

The normal path is:

```
1. Create an MLSys profile / account.
2. Register as an attendee.
3. Pay the registration fee.
4. Receive receipt / QR code by email.
5. Bring ID and check in at the registration desk.
```

Direct links:

| Action | Link |
|---|---|
| Registration portal | https://mlsys.org/Register/view-registration |
| Alternate registration URL | https://mlsys.org/Register2 |
| Pricing page | https://mlsys.org/Conferences/2026/Pricing |
| Conference hotel | https://mlsys.org/Conferences/2026/HotelInformation |
| Contact organizers | https://mlsys.org/Help/Contact |

Important: the “Register as an attendee” step appears **after** you create a profile or log in. The public page shows Step 1 first:

```
1. Login or Create a new Profile
2. Register
3. Payment and Receipt
```

So if you cannot see attendee options yet, first create the profile with email, name, institution, country, and diet. Then click **Next**. The registration choices should appear in Step 2.

You need a paper only if you want to present research. You need an invitation letter mostly for visa/travel documentation. If you live nearby and do not need a visa, you should not need an invitation letter.

What to check before going:

```
- Is in-person registration still open?
- Is there a student / regular / industry attendee category that fits you?
- Does registration include all days or one-day access?
- Are workshops, tutorials, meals, or receptions included?
- Is there an onsite registration option if online registration has issues?
```

If unsure, contact the organizers from the MLSys contact page and ask:

```
Hi, I live near Bellevue and would like to attend MLSys 2026 as a regular attendee.
I do not have a submitted paper or invitation. Can I register and attend the talks
and poster sessions?
```

Most likely answer: yes, if registration capacity is available.

---

## 3. Best Learning Strategy

Do not try to understand every paper. MLSys is dense.

Use this strategy:

```
Before conference:
  Pick 3 tracks.
  Read abstracts only.
  Write down questions.

During conference:
  Attend keynotes.
  Attend oral sessions for your tracks.
  Spend serious time at posters.
  Ask authors: “What problem does this solve in production?”

After conference:
  Pick 5 papers to study deeply.
  Add notes into this repo.
```

The highest ROI is usually **poster sessions**, because you can talk directly to authors.

---

## 4. Tracks to Prioritize for This Repo

### Track A — LLM Serving

Why it matters:

```
Serving is where model quality meets real-world latency, cost, and reliability.
```

Look for papers about:

- KV cache management
- prefill/decode disaggregation
- speculative decoding
- batching and scheduling
- SLO-aware serving
- serverless inference
- heterogeneous GPUs
- cold start latency
- vLLM / SGLang-style systems

Relevant repo folders:

```
key-technologies/ml-serving/vllm/
key-technologies/ml-serving/sglang/
key-technologies/ml-serving/tensorrt-llm/
ml-sys/paged-attention/
```

Questions to ask authors:

```
- What is the bottleneck: prefill, decode, memory, network, or scheduling?
- What workload distribution did you evaluate?
- Does it work for long context and multi-turn chat?
- Does it require changes to the model, runtime, or scheduler?
- What breaks at production scale?
```

---

### Track B — LLM Training

Why it matters:

```
Training is where distributed systems, fault tolerance, memory, and communication dominate.
```

Look for papers about:

- FSDP / ZeRO-style sharding
- context parallelism
- MoE training
- heterogeneous GPU clusters
- straggler detection
- training reliability
- memory management
- FP8 training
- low-bandwidth distributed training

Relevant repo folders:

```
key-technologies/ml-training/deepspeed/
key-technologies/ml-training/megatron/
key-technologies/ml-training/jax/
key-technologies/ml-training/nccl/
key-technologies/ml-training/slurm/
key-technologies/ml-training/large-scale-training/
```

Questions to ask authors:

```
- What parallelism dimensions are used: DP, TP, PP, CP, EP?
- What communication collective dominates runtime?
- How do they handle failures and stragglers?
- What hardware assumptions are required?
- Does the system improve MFU or just reduce OOMs?
```

---

### Track C — Agentic AI Systems

Why it matters:

```
AI systems are moving from single-request inference to multi-step agents.
```

Look for papers about:

- agent memory
- multi-agent execution
- computer-use agents
- agentic security
- agent SDKs
- RAG agents
- tool-use scheduling
- agent inference optimization

Relevant repo folders:

```
key-technologies/ml-models/
key-technologies/ml-serving/sglang/
key-technologies/ml-training/rl-frameworks/
real-designs/28-rl-platform/
```

Questions to ask authors:

```
- What is the agent runtime architecture?
- How are tool calls scheduled and validated?
- How is memory represented and retrieved?
- What are the failure modes: loops, stale context, hallucinated actions?
- How do they evaluate long-horizon success?
```

---

### Track D — Model Compression and Efficient Computation

Why it matters:

```
Compression is how frontier models become affordable to serve.
```

Look for papers about:

- quantization
- sparse activation
- KV cache quantization
- FP8 / NVFP4
- entropy coding
- attention sparsity
- edge inference
- TinyML

Relevant repo folders:

```
ml-sys/quantization/
ml-sys/flash-attention/
key-technologies/ml-compilers/triton/
key-technologies/ml-compilers/tensorrt/
```

Questions to ask authors:

```
- What accuracy metric drops, and by how much?
- Is the method model-specific or general?
- Does it reduce memory, bandwidth, latency, or all three?
- Does it require custom kernels?
- Does it work with batching and long context?
```

---

## 5. Keynotes / Invited Talks to Prioritize

From the 2026 program, especially relevant talks include:

| Talk | Why Attend |
|---|---|
| Beyond Model Serving: Cross-Stack Co-Design for Agentic Systems | Directly matches agentic systems and serving trends |
| LMCache: An Efficient KV Cache Layer for Enterprise-Scale LLM Inference | KV cache is one of the central serving bottlenecks |
| Eliciting Language Model Behaviors with Investigator Agents | Connects evaluation, safety, prompting, and RL-style behavior elicitation |
| When AI Starts Writing Systems Code | Important for AI-generated kernels and systems optimization |
| Rethinking Pretraining: Data and Architecture | Important for model training and data/architecture scaling |
| The Next Horizon of Systems: From MLSys to System Intelligence | Big-picture direction of AI changing systems engineering |
| Amin Vahdat keynote | Google-scale AI infrastructure perspective |
| Christos Kozyrakis keynote | Hardware/systems perspective from warehouse-scale to efficient compute |

---

## 6. One-Day Plan If You Can Only Attend One Day

If only one day is possible, prioritize **Wednesday May 20** or **Thursday May 21**.

### Wednesday May 20

Good for:

- LLM serving
- LLM training
- RL for LLMs
- model compression
- MoE serving

High-value sessions:

```
Morning:
  R2: LLM Serving
  R6: LLM Training
  Keynote: Amin Vahdat

Afternoon:
  R3: LLM Serving
  R12: LLM Training, includes RL-related LLM papers
  R15: Model Compression
  Poster Session 2
```

### Thursday May 21

Good for:

- agentic AI
- industry systems
- serving
- benchmarks
- compilers/kernels
- hardware

High-value sessions:

```
Morning:
  I5: Agentic AI/MLSys
  R13: LLM Serving
  Keynote: Luke Zettlemoyer

Afternoon:
  I1: LLM Serving
  R18: Benchmarks
  I2: LLM Training
  R4: Compilers and Kernels
  Poster Session 3
```

---

## 7. Poster Session Playbook

At a poster, ask short, practical questions:

```
1. What exact bottleneck were you solving?
2. What was the baseline?
3. What is the real production constraint?
4. What assumption would break the method?
5. What would you do differently if you had another 6 months?
6. Is the code open source?
7. Which prior paper should I read first?
```

Do not ask vague questions like “Can you explain your paper?”

Better:

```
“I’m learning LLM serving. Is your main win from memory reduction, better scheduling, or fewer decode steps?”
```

---

## 8. Papers / Topics to Hunt For

Based on the 2026 program, search the paper list for these keywords:

```
LLM Serving:
  KV cache, prefill, decode, speculative decoding, vLLM, SGLang,
  disaggregation, batching, SLO, cold start, serverless

Training:
  FSDP, context parallelism, MoE, FP8, heterogeneous clusters,
  stragglers, fault tolerance, memory management

RL / Reasoning:
  RL training, RLVR, verifier, speculative decoding for RL,
  heterogeneous environments, reward, policy

Agents:
  agentic, OpenHands, computer-use agents, memory, RAG,
  multi-agent, agent security

Compression:
  quantization, NVFP4, FP8, sparse activation, KV quantization,
  entropy coding, TinyML

Compilers / Kernels:
  Triton, GPU kernels, FlashAttention, MLIR, compiler autotuning,
  operator scheduling
```

---

## 9. Networking Script

Simple introduction:

```
Hi, I’m studying ML systems deeply — distributed training, serving,
RLHF, and GPU systems. I’m building a learning lab and trying to understand
what problems people are actually solving in production.
```

Good follow-up:

```
If I want to reproduce the core idea from your paper, what is the smallest
experiment I should implement first?
```

Ask for pointers:

```
What are 2-3 papers I should read before trying to understand this area deeply?
```

---

## 10. After-Conference Checklist

```
- Write a one-page summary for each attended session.
- Pick 5 papers to read deeply.
- For each paper, capture:
  - problem
  - baseline
  - key idea
  - system design
  - metrics
  - limitations
  - repo link, if any
- Add follow-up notes into relevant folders in this lab.
```

Suggested follow-up folders:

```
key-technologies/ml-serving/
key-technologies/ml-training/
key-technologies/ml-compilers/
key-technologies/ml-models/
ml-sys/
real-designs/28-rl-platform/
```

---

## 11. Bottom Line

```
If you live near Bellevue, MLSys 2026 is absolutely worth attending.

Best value:
  1. keynotes for big picture
  2. oral sessions for trend discovery
  3. posters for real learning and networking
  4. sponsor booths for production tooling

Goal:
  Do not try to learn everything.
  Come back with 5 deep topics and 10 good paper leads.
```
