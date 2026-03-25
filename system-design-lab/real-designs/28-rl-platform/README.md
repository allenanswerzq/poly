# Design a Large-Scale RL Platform (like OpenAI's RLHF / DeepMind's AlphaGo infra)

## Problem Statement

Design a platform for training reinforcement learning agents at scale — specifically RLHF (Reinforcement Learning from Human Feedback) for LLMs, and general RL for game/robotics agents. The core challenge: RL requires tight integration between data generation (rollouts), training, and evaluation, all running in parallel across thousands of GPUs.

## Why RL Is Architecturally Different

```
Supervised learning:
  Fixed dataset → train on batches → done
  Simple: data doesn't change, no feedback loop

Reinforcement learning:
  Generate data (rollouts) → train on it → generate NEW data → train → ...
  Complex: data generation and training are coupled in a loop

  ┌──────────┐     ┌──────────┐     ┌──────────┐
  │ Generate  │────►│  Train   │────►│ Generate  │────► ...
  │ rollouts  │     │ (update  │     │ rollouts  │
  │           │◄────│  policy) │◄────│ (new      │
  └──────────┘     └──────────┘     │  policy)  │
                                     └──────────┘
  Data generation needs the LATEST model
  Training needs FRESH rollout data
  → tight coupling, pipeline bubbles, synchronization headaches
```

## RLHF Architecture (LLM alignment)

```
┌─────────────────────────────────────────────────────────────────┐
│                    RLHF Training Pipeline                        │
│                                                                  │
│  Phase 1: Generate responses (Actor / Policy model)              │
│  ┌────────────────────────────────────┐                         │
│  │ Prompts ──► LLM (Policy) ──► Responses                       │
│  │ 1000 prompts, generate 4 responses each                      │
│  │ Run on: 64 GPUs (inference mode, vLLM for speed)             │
│  └────────────────────────────────────┘                         │
│              │                                                   │
│              ▼                                                   │
│  Phase 2: Score responses (Reward Model)                         │
│  ┌────────────────────────────────────┐                         │
│  │ (prompt, response) ──► Reward Model ──► scalar score          │
│  │ "How good is this response?" (0-10)                          │
│  │ Run on: 16 GPUs (inference mode)                             │
│  └────────────────────────────────────┘                         │
│              │                                                   │
│              ▼                                                   │
│  Phase 3: PPO Training (update policy)                           │
│  ┌────────────────────────────────────┐                         │
│  │ Advantages = rewards - baseline                               │
│  │ Loss = -advantage × log_prob(response) + KL penalty           │
│  │ Update policy model weights via gradient descent              │
│  │ Run on: 64 GPUs (training mode, FSDP)                        │
│  └────────────────────────────────────┘                         │
│              │                                                   │
│              ▼                                                   │
│  Repeat: use updated policy to generate new responses            │
└─────────────────────────────────────────────────────────────────┘
```

### The 4 Models in RLHF

```
┌─────────────────────────────────────────────────────┐
│ Model          │ Purpose              │ GPU Mode     │
├────────────────┼──────────────────────┼──────────────┤
│ Policy (Actor) │ Generate responses   │ Inference    │
│ Reference      │ KL divergence anchor │ Inference    │
│ Reward         │ Score responses      │ Inference    │
│ Critic (Value) │ Estimate baselines   │ Training     │
├────────────────┼──────────────────────┼──────────────┤
│ Total GPU mem  │ 4 × 70B model ≈      │ ~320 GPUs    │
│ (70B LLM)      │ 560GB+ per model    │ (A100 80GB)  │
└─────────────────────────────────────────────────────┘
```

## General RL Architecture (Game / Robotics)

```
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│   ┌──────────────────────────────────────────────┐          │
│   │          Rollout Workers (CPU/GPU)            │          │
│   │                                               │          │
│   │  Worker 0: env → obs → policy → action → env  │          │
│   │  Worker 1: env → obs → policy → action → env  │          │
│   │  ...                                          │          │
│   │  Worker 999: env → obs → policy → ...          │          │
│   │                                               │          │
│   │  Each worker runs an environment instance     │          │
│   │  Collects (state, action, reward) trajectories│          │
│   └──────────────────┬───────────────────────────┘          │
│                      │ trajectories                          │
│                      ▼                                       │
│   ┌──────────────────────────────────────────────┐          │
│   │          Replay Buffer / Experience Store      │          │
│   │                                               │          │
│   │  Ring buffer of (s, a, r, s') tuples           │          │
│   │  Prioritized sampling (prioritized experience  │          │
│   │  replay — sample rare important experiences)   │          │
│   │  Size: millions of transitions                │          │
│   └──────────────────┬───────────────────────────┘          │
│                      │ sample batches                        │
│                      ▼                                       │
│   ┌──────────────────────────────────────────────┐          │
│   │          Learner (GPU Training)               │          │
│   │                                               │          │
│   │  Sample batch from replay buffer              │          │
│   │  Compute loss (PPO / SAC / DQN)               │          │
│   │  Update policy weights                        │          │
│   │  Push new weights to rollout workers           │          │
│   │  Run on: 8-64 GPUs (data parallel training)   │          │
│   └──────────────────┬───────────────────────────┘          │
│                      │ updated weights                       │
│                      ▼                                       │
│   ┌──────────────────────────────────────────────┐          │
│   │          Evaluator                            │          │
│   │                                               │          │
│   │  Run policy on eval environments              │          │
│   │  Track reward, win rate, episode length       │          │
│   │  Decide when to checkpoint                    │          │
│   └──────────────────────────────────────────────┘          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Key Challenges & Solutions

### 1. Generation-Training Pipeline (RLHF)

```
Problem: generation and training use the same model but in different modes
  Generation: inference mode (no gradients, batched)
  Training: training mode (gradients, FSDP)

Option A: Separate clusters (most common today)
  64 GPUs for generation (inference) + 64 GPUs for training
  Pros: simple, no mode switching
  Cons: expensive (128 GPUs), idle GPUs during the other phase

Option B: Colocated (same GPUs switch modes)
  64 GPUs: generate → switch to training mode → train → switch back
  Pros: half the GPUs needed
  Cons: mode switching overhead, pipeline bubbles

Option C: Hybrid (overlapped)
  Generation and training overlapped in pipeline
  While training on batch N, generate batch N+1
  Pros: highest GPU utilization
  Cons: complex scheduling, stale policy in generation
```

### 2. Weight Synchronization

```
After training step, new weights must reach all rollout workers.

Small model (1B params, 2GB):
  Broadcast weights → 2GB over network → ~1 second
  Acceptable: workers use slightly stale weights for 1 second

Large model (70B params, 140GB):
  Broadcast → 140GB over network → ~30 seconds with InfiniBand
  Problem: workers idle for 30 seconds!

Solution: streaming weight updates
  Send weight diffs (delta) instead of full weights
  Or: use NCCL broadcast with pipelining
  Or: overlap weight transfer with rollout computation
```

### 3. Reward Model Bottleneck (RLHF)

```
Reward model scores every generated response.
1000 prompts × 4 responses = 4000 reward model calls.
If each call takes 100ms → 400 seconds sequentially!

Solution: batch reward model inference
  Dynamic batching with vLLM → 4000 calls in ~10 seconds
  Run reward model on dedicated inference GPUs
  Pipeline: while training on scored data, score the next batch
```

### 4. Sample Efficiency

```
RL is notoriously sample-inefficient:
  On-policy (PPO): use data once, then throw away
    → need to generate TONS of data
    → 1000s of rollout workers

  Off-policy (SAC, DQN): store data in replay buffer, reuse many times
    → more efficient, fewer workers needed
    → but: stale data can cause instability

RLHF uses PPO (on-policy) → needs many rollout GPUs
Recent trend: GRPO, DPO (skip RL entirely, use preference data directly)
  → Simpler, more efficient, but less flexible
```

## Infrastructure Numbers (RLHF for 70B LLM)

```
Generation (vLLM inference):
  64 × A100 80GB (8 nodes × 8 GPUs)
  Throughput: ~50K tokens/sec

Reward Model:
  16 × A100 80GB (2 nodes)
  Throughput: ~1000 scores/sec

PPO Training (FSDP):
  64 × A100 80GB (8 nodes)
  1 training step: ~30 seconds

Full RLHF iteration:
  Generate 4000 responses: ~60 seconds
  Score with reward model: ~10 seconds
  PPO training step:       ~30 seconds
  Weight sync:             ~5 seconds
  Total: ~105 seconds per iteration

  1000 iterations → ~29 hours of training
  Total GPUs: 144 × A100 (~$400/hour) → ~$11K for the run
```

## Platform Components (Ray-based)

```python
# Typical RLHF implementation uses Ray for orchestration
import ray

@ray.remote(num_gpus=8)
class PolicyGenerator:
    """Generate responses using vLLM on 8 GPUs."""
    def generate(self, prompts):
        return self.vllm_engine.generate(prompts)

@ray.remote(num_gpus=2)
class RewardScorer:
    """Score responses with reward model."""
    def score(self, prompt_responses):
        return self.reward_model.predict(prompt_responses)

@ray.remote(num_gpus=8)
class PPOTrainer:
    """Update policy with PPO on 8 GPUs (FSDP)."""
    def train_step(self, rollouts, scores):
        loss = self.ppo_loss(rollouts, scores)
        loss.backward()
        self.optimizer.step()
        return self.get_weights()

# Orchestrator
for iteration in range(1000):
    responses = generator.generate.remote(prompts)
    scores = scorer.score.remote(responses)
    new_weights = trainer.train_step.remote(responses, scores)
    generator.update_weights.remote(new_weights)
```

## Interview Talking Points

> "The RLHF pipeline has three phases: generation, scoring, and training. The bottleneck is generation — generating diverse responses with a 70B model on 64 GPUs. We use vLLM for generation throughput and overlap scoring with the next batch of generation. Weight sync after each PPO step takes ~5 seconds via NCCL broadcast across NVLink."

> "For general RL, the architecture is an actor-learner split. 1000 rollout workers (CPU) collect trajectories in parallel, store in a prioritized replay buffer, and the learner (8 GPUs) samples batches for training. We use Ray to orchestrate workers and handle fault tolerance — if a worker dies, Ray automatically restarts it."

> "Sample efficiency is the key cost driver. PPO (on-policy) requires fresh data every iteration, so we need many rollout GPUs. Recent methods like DPO skip RL entirely and train directly on preference pairs, which is 10x cheaper but less flexible for reward shaping."
