# Reinforcement Learning Frameworks — Comprehensive Study

> RL frameworks are a big missing piece. Supervised learning frameworks train on a fixed dataset; RL frameworks must coordinate **environment simulation**, **rollout collection**, **policy inference**, **reward computation**, **experience storage**, **training**, and **evaluation** in a feedback loop.

---

## 1. The Big Picture

```
RL is not one framework category. It is a stack:

  ┌──────────────────────────────────────────────────────────────┐
  │ Application layer                                            │
  │   Games, robotics, trading, recommendation, RLHF, agents     │
  ├──────────────────────────────────────────────────────────────┤
  │ Algorithm layer                                              │
  │   DQN, PPO, SAC, A3C, IMPALA, MuZero, AlphaZero, GRPO, DPO   │
  ├──────────────────────────────────────────────────────────────┤
  │ Training framework layer                                     │
  │   Stable-Baselines3, CleanRL, RLlib, TorchRL, Tianshou, TRL   │
  ├──────────────────────────────────────────────────────────────┤
  │ Distributed execution layer                                  │
  │   Ray, multiprocessing, Kubernetes, Slurm, actor pools        │
  ├──────────────────────────────────────────────────────────────┤
  │ Environment / simulator layer                                │
  │   Gymnasium, PettingZoo, Brax, MuJoCo, Isaac Lab, EnvPool     │
  └──────────────────────────────────────────────────────────────┘
```

The important mental model:

```
Supervised learning:
  dataset -> train -> checkpoint

Reinforcement learning:
  policy -> act in environment -> collect trajectories -> train policy -> repeat

LLM RLHF / reasoning RL:
  prompts -> generate responses -> score / verify -> optimize policy -> repeat
```

---

## 2. Why RL Frameworks Are Different

```
RL workloads combine THREE systems problems:

1. Simulation throughput
   Need many environment steps per second.
   Example: Atari, MuJoCo, robot simulation, browser agents.

2. Training stability
   RL data is non-stationary because the policy changes while data is collected.
   Yesterday's data came from yesterday's policy.

3. Distributed coordination
   Rollout workers, learners, replay buffers, evaluators, reward models,
   checkpointing, and dashboards all run together.
```

Classic bottlenecks:

```
Environment bottleneck:
  GPU learner waits because CPU simulators are slow.

Policy inference bottleneck:
  Thousands of actors need actions from the latest policy.

Replay bottleneck:
  Experience store must handle high write throughput and prioritized sampling.

Reward bottleneck:
  RLHF reward model or verifier must score every generated sample.

Synchronization bottleneck:
  New policy weights must be broadcast to rollout workers.
```

---

## 3. Timeline of Important RL Frameworks and Systems

| Year | Framework / System | Category | Main Contribution |
|---:|---|---|---|
| 1998 | TD-Gammon influence | Classic RL system | Early neural RL success in backgammon |
| 2013 | DQN | Deep RL algorithm | CNN + Q-learning + replay buffer for Atari |
| 2015 | OpenAI Gym | Environment API | Standardized `reset()` / `step()` interface |
| 2015 | TensorFlow Agents roots | Algorithm library | TensorFlow-based RL components |
| 2016 | A3C | Distributed RL algorithm | Many actor-learners asynchronously train one policy |
| 2017 | Ray / RLlib | Distributed RL framework | Actor-based distributed RL at scale |
| 2017 | Dopamine | Research RL framework | Small, reproducible value-based RL baselines |
| 2018 | IMPALA / SEED RL | Distributed RL architecture | Decoupled actors and learners; high-throughput RL |
| 2018 | TF-Agents | RL library | TensorFlow RL toolkit with policies, drivers, replay buffers |
| 2019 | Acme | Research RL framework | DeepMind-style modular distributed agents |
| 2019 | Brax | Differentiable simulator | JAX-native physics simulation on accelerator hardware |
| 2020 | Stable-Baselines3 | Practical RL library | Clean PyTorch implementations of common algorithms |
| 2020 | Tianshou | Modular PyTorch RL | Flexible collectors, policies, replay buffers |
| 2021 | CleanRL | Single-file implementations | Readable, reproducible RL algorithms |
| 2021 | PettingZoo | Multi-agent env API | Standard API for multi-agent RL environments |
| 2021 | EnvPool | Vectorized environments | Very fast C++ environment execution behind Gym APIs |
| 2022 | TorchRL | PyTorch-native RL stack | RL components integrated with PyTorch ecosystem |
| 2022 | TRL | LLM alignment library | PPO/DPO-style training for transformer language models |
| 2023 | OpenRLHF / DeepSpeed-Chat | LLM RLHF systems | Scalable RLHF pipelines for large language models |
| 2024 | verl | LLM RL training system | Flexible RLHF/GRPO/PPO pipelines with distributed generation/training |
| 2024 | Isaac Lab | Robotics RL simulation | GPU-accelerated robot simulation and training workflows |
| 2025+ | Reasoning RL pipelines | LLM reasoning / agents | RL with verifiers, tool-use rewards, GRPO/RLOO-style objectives |

---

## 4. Framework Map by Use Case

### 4.1 Environment APIs

| Framework | Best For | Core Idea | Notes |
|---|---|---|---|
| Gymnasium | Single-agent RL environments | Standard `reset()` / `step()` API | Successor to OpenAI Gym; default environment interface |
| PettingZoo | Multi-agent RL | Agent-environment cycle / parallel multi-agent APIs | Used for MARL research and benchmarks |
| dm_env | DeepMind environments | TimeStep object API | Common in DeepMind Acme / JAX ecosystem |
| EnvPool | Fast vectorized environments | C++ env execution, Python API | Useful when simulator speed is the bottleneck |
| Brax | JAX physics simulation | Accelerator-native differentiable physics | Great for JAX + TPU/GPU RL experiments |
| MuJoCo | Physics simulation | Continuous-control robotics simulation | Classic robotics/control benchmark engine |
| Isaac Lab / Isaac Gym | Robot simulation at scale | GPU-accelerated physics for many parallel envs | Good for sim-to-real and embodied AI research |
| OpenSpiel | Games and multi-agent learning | Game-theoretic environments | Poker, board games, imperfect information games |

### 4.2 Algorithm Libraries

| Framework | Language / Backend | Best For | Strength | Weakness |
|---|---|---|---|---|
| Stable-Baselines3 | PyTorch | Practical single-machine RL | Easy PPO/SAC/DQN/A2C training | Not built for huge distributed RL |
| CleanRL | PyTorch/JAX variants | Learning and reproducibility | Single-file algorithms, very readable | Less framework abstraction; fewer batteries included |
| RLlib | Ray + PyTorch/TF | Distributed RL | Scales actors/learners with Ray | More complex; heavier abstraction |
| Tianshou | PyTorch | Modular research RL | Flexible collectors and replay buffers | Smaller ecosystem than SB3/RLlib |
| TorchRL | PyTorch | PyTorch-native RL building blocks | Integrates with tensors, transforms, distributed PyTorch | Still lower-level than SB3 |
| Acme | JAX/TF | Research-grade agents | Modular actor/learner architecture | DeepMind-style; steeper learning curve |
| Dopamine | JAX/TF | Value-based Atari research | Reproducible DQN/Rainbow-style baselines | Narrower algorithm scope |
| Sample Factory | PyTorch | High-throughput on-policy RL | Fast PPO-style training, games/simulators | Less general than RLlib |
| skrl | PyTorch/JAX | Robotics and simulation RL | Good Isaac Gym/Isaac Lab integration | Robotics-focused |

### 4.3 LLM Alignment / RLHF Frameworks

| Framework | Best For | Algorithms / Features | Notes |
|---|---|---|---|
| TRL | Hugging Face LLM alignment | SFT, PPO, DPO, reward modeling | Easiest entry point for small/medium LLM alignment |
| OpenRLHF | Scalable open RLHF | PPO, DPO, reward models, Ray/vLLM-style generation | Designed for large-model RLHF workflows |
| DeepSpeed-Chat | DeepSpeed RLHF pipeline | SFT, reward model, PPO | Early end-to-end RLHF reference on DeepSpeed |
| NeMo-Aligner | NVIDIA NeMo alignment | SFT, reward model, PPO/DPO-style alignment | Strong if already using NeMo/Megatron stack |
| verl | Distributed LLM RL | PPO, GRPO-style flows, flexible rollout/training separation | Important for reasoning-model and RLHF experiments |
| trlX | Earlier distributed RLHF | PPO for language models | Historically important; many ideas moved into newer stacks |
| Axolotl | Fine-tuning workflows | SFT, preference tuning integrations | More fine-tuning orchestration than RL system |

---

## 5. RL Algorithms You Must Know

### 5.1 Value-Based RL

```
Main idea:
  Learn Q(s, a): expected return if you take action a in state s.

Canonical algorithms:
  - Q-learning
  - DQN
  - Double DQN
  - Dueling DQN
  - Rainbow DQN

Best for:
  - Discrete action spaces
  - Atari-style environments
  - Cases where action enumeration is possible

Core infrastructure:
  - Replay buffer
  - Target network
  - Epsilon-greedy exploration
```

### 5.2 Policy Gradient RL

```
Main idea:
  Directly optimize the policy π(a | s).

Canonical algorithms:
  - REINFORCE
  - A2C / A3C
  - PPO
  - TRPO

Best for:
  - Large or continuous action spaces
  - Robotics/control
  - RLHF and language model policies

Core infrastructure:
  - Rollout workers
  - Advantage estimation
  - Policy/value networks
  - KL or clipping constraints
```

### 5.3 Actor-Critic RL

```
Main idea:
  Actor chooses actions; critic estimates values.

Canonical algorithms:
  - A2C / A3C
  - PPO
  - DDPG
  - TD3
  - SAC

Best for:
  - Continuous control
  - Simulated robotics
  - Stable policy optimization
```

### 5.4 Model-Based RL and Planning

```
Main idea:
  Learn or use a model of the environment, then plan.

Canonical systems:
  - AlphaGo / AlphaZero
  - MuZero
  - Dreamer

Best for:
  - Games
  - Planning-heavy tasks
  - Simulation-rich domains

Core infrastructure:
  - Self-play workers
  - Search / MCTS
  - Replay buffer
  - Policy/value/reward models
```

### 5.5 LLM Alignment and Reasoning RL

```
Main idea:
  Treat the LLM as a policy over tokens.

Classic RLHF:
  prompt -> response -> reward model score -> PPO update

Modern preference / reasoning variants:
  - DPO: direct preference optimization, no online RL loop
  - KTO: binary good/bad feedback
  - RLOO: leave-one-out baselines for multiple sampled responses
  - GRPO: group-relative advantages, often no separate value model
  - Verifier RL: use math/code/unit-test/verifier reward signals

Why this matters now:
  Reasoning models are often trained with reward signals that grade answers,
  intermediate traces, tool use, code execution, tests, or final outcomes.
```

---

## 6. Architecture Patterns

### 6.1 Single-Machine RL

```
Use this for:
  - Learning RL
  - Small Gymnasium tasks
  - Prototyping algorithms
  - Local robotics simulation

Typical stack:
  Gymnasium + Stable-Baselines3 / CleanRL + Weights & Biases

Flow:
  ┌─────────────┐
  │ Environment │
  └──────┬──────┘
         │ obs, reward
         ▼
  ┌─────────────┐
  │   Policy    │
  └──────┬──────┘
         │ action
         ▼
  ┌─────────────┐
  │  Env step   │
  └─────────────┘
```

### 6.2 Vectorized Environments

```
Problem:
  One environment is too slow.

Solution:
  Run N environments in parallel and batch policy inference.

  env_0 ─┐
  env_1 ─┤
  env_2 ─┼── batch obs -> policy -> batch actions
  ...   ─┤
  env_N ─┘

Used by:
  Stable-Baselines3, Gymnasium vector envs, EnvPool, Sample Factory.
```

### 6.3 Actor-Learner Architecture

```
Use this for:
  - Large-scale RL
  - Games
  - Robotics simulation
  - High-throughput training

  ┌──────────────┐       trajectories       ┌──────────────┐
  │ Actor pool   │─────────────────────────►│ Replay /     │
  │ env + policy │                          │ trajectory   │
  │ workers      │◄──── latest weights ─────│ store        │
  └──────────────┘                          └──────┬───────┘
                                                    │ batches
                                                    ▼
                                             ┌──────────────┐
                                             │ Learner GPU  │
                                             │ train policy │
                                             └──────────────┘
```

Key design questions:

```
- Are actors synchronous or asynchronous?
- How stale can policy weights be?
- Is the replay buffer on-policy or off-policy?
- Are environments CPU-bound, GPU-bound, or remote services?
- Is training bottlenecked by simulation, inference, reward scoring, or gradient steps?
```

### 6.4 RLHF / LLM RL Architecture

```
LLM RL has a different shape:

  ┌─────────┐    ┌──────────────┐    ┌──────────────┐
  │ Prompts │───►│ Policy LLM   │───►│ Responses    │
  └─────────┘    │ generation   │    └──────┬───────┘
                 └──────────────┘           │
                                            ▼
                                     ┌──────────────┐
                                     │ Reward model │
                                     │ or verifier  │
                                     └──────┬───────┘
                                            │ rewards
                                            ▼
                                     ┌──────────────┐
                                     │ PPO / GRPO / │
                                     │ DPO training │
                                     └──────────────┘
```

Systems components:

```
- Policy model: generates responses or trajectories
- Reference model: KL anchor to avoid policy drift
- Reward model / verifier: scores output quality
- Critic model: value baseline for PPO-style methods
- Inference engine: vLLM / SGLang / TensorRT-LLM for generation
- Training engine: FSDP / DeepSpeed / Megatron / JAX sharding
- Scheduler: Ray, Kubernetes, Slurm, custom orchestration
```

---

## 7. Framework Comparison: What Should You Use?

| Goal | Recommended Stack | Why |
|---|---|---|
| Learn RL basics | Gymnasium + CleanRL | Minimal code, easy to inspect |
| Train common RL baselines quickly | Gymnasium + Stable-Baselines3 | Practical, documented, reliable |
| Distributed RL at cluster scale | Ray RLlib | Actor model maps naturally to rollout workers |
| Robotics simulation | Isaac Lab + skrl / RLlib / TorchRL | GPU simulation and robotics tooling |
| JAX-based RL research | Brax + JAX + Flax/Optax, or Acme | Accelerator-native simulation/training |
| Multi-agent RL | PettingZoo + RLlib / Tianshou | Multi-agent APIs and algorithms |
| Atari/value-based research | Dopamine / CleanRL | Reproducible DQN-family baselines |
| LLM PPO / DPO experiments | Hugging Face TRL | Easy entry point and transformer integration |
| Large-scale LLM RLHF | OpenRLHF / verl / NeMo-Aligner | Handles rollout generation + distributed training |
| Reasoning model RL | verl / OpenRLHF + verifiers + vLLM/SGLang | Flexible generation/reward/training loop |

Decision tree:

```
Are you doing LLM alignment / reasoning RL?
  yes -> TRL for simple; OpenRLHF / verl / NeMo-Aligner for scale.
  no  -> continue.

Need distributed rollout workers?
  yes -> Ray RLlib, Acme, Sample Factory, or custom Ray.
  no  -> continue.

Need robotics simulation?
  yes -> Isaac Lab or MuJoCo/Brax.
  no  -> continue.

Want easiest practical baseline?
  Stable-Baselines3.

Want to understand every line?
  CleanRL.

Want JAX accelerator-native simulation?
  Brax.
```

---

## 8. RL Frameworks vs Training Frameworks

```
PyTorch / JAX / TensorFlow:
  Tensor math + autodiff + neural network training.

RL framework:
  Everything around the neural net:
    - environment interaction
    - rollout collection
    - replay buffers
    - policy evaluation
    - advantage computation
    - distributed actors
    - reward model scoring
    - checkpoint/eval loops

Ray:
  Distributed execution substrate.
  RLlib uses Ray actors to run environments and learners.

JAX:
  Great for pure functional policies, jit, vmap, pmap/sharding.
  Great when the environment can also run in JAX, like Brax.

PyTorch:
  Best ecosystem for practical RL libraries and robotics tooling.
```

---

## 9. Key Metrics to Track

| Metric | Meaning | Why It Matters |
|---|---|---|
| Environment steps/sec | Simulation throughput | Main throughput metric for classic RL |
| Samples/sec | Rollout or token generation rate | Main throughput metric for RLHF and reasoning RL |
| Learner updates/sec | Gradient update speed | Shows if training is bottlenecked |
| Policy lag | How stale actor weights are | Too much lag destabilizes RL |
| Replay age | How old sampled experience is | Old data may hurt on-policy methods |
| Reward latency | Time to score trajectories | Reward model/verifier can become bottleneck |
| KL to reference | LLM policy drift | Prevents reward hacking and language degradation |
| Evaluation reward | True held-out performance | Training reward can be misleading |
| Wall-clock to target | Time to reach score | Best practical system metric |
| GPU utilization | Hardware efficiency | RL often has low utilization without batching/pipelining |

---

## 10. Common Failure Modes

```
Reward hacking:
  Policy exploits reward model bugs instead of solving the real task.

Policy collapse:
  Policy becomes deterministic or repetitive too early.

Training instability:
  Rewards spike then crash because updates are too aggressive.

Stale rollouts:
  Actors generate data from old policies while learner has moved on.

Simulator overfitting:
  Agent learns quirks of simulator, fails in real world.

Bad exploration:
  Agent never discovers high-reward states.

Evaluation leakage:
  Reward/verifier or benchmark accidentally leaks target answers.

LLM alignment drift:
  RL improves one metric but hurts helpfulness, safety, formatting, or language quality.
```

Mitigations:

```
- Keep a frozen reference policy and KL penalty.
- Use held-out evals and adversarial evals.
- Track both reward model score and human/ground-truth metrics.
- Use conservative PPO clipping or preference objectives like DPO/GRPO.
- Version environments, prompts, reward models, and datasets.
- Log trajectories, not just scalar metrics.
- Run ablations: reward model off, KL off, replay on/off, stale policy thresholds.
```

---

## 11. Study Plan

### Phase 1 — RL Basics

```
1. Understand MDPs: state, action, reward, transition, discount.
2. Implement tabular Q-learning.
3. Train DQN on CartPole or Atari-like tasks.
4. Learn replay buffers and target networks.
```

Recommended stack:

```
Gymnasium + CleanRL
```

### Phase 2 — Practical Deep RL

```
1. Train PPO, SAC, DQN with Stable-Baselines3.
2. Compare on-policy vs off-policy methods.
3. Learn vectorized environments.
4. Track runs with TensorBoard or Weights & Biases.
```

Recommended stack:

```
Gymnasium + Stable-Baselines3 + EnvPool
```

### Phase 3 — Distributed RL

```
1. Learn Ray actors and tasks.
2. Run RLlib PPO with many rollout workers.
3. Study actor-learner architectures: A3C, IMPALA, SEED RL.
4. Measure environment steps/sec and policy lag.
```

Recommended stack:

```
Ray RLlib
```

### Phase 4 — JAX RL

```
1. Learn pure functional policies in JAX.
2. Train on Brax environments.
3. Use jit/vmap to batch environments and policy evaluation.
4. Compare GPU/TPU throughput vs Python environment loops.
```

Recommended stack:

```
JAX + Brax + Flax/Optax
```

### Phase 5 — RLHF / LLM RL

```
1. Understand SFT -> reward model -> PPO.
2. Train a tiny reward model on preference pairs.
3. Run DPO with TRL.
4. Study PPO/GRPO with OpenRLHF or verl.
5. Add verifier rewards for math/code tasks.
```

Recommended stack:

```
TRL for learning.
OpenRLHF / verl for scale.
```

### Phase 6 — RL Platform Design

```
1. Study rollout workers, replay buffers, learners, evaluators.
2. Design weight synchronization.
3. Design reward model serving.
4. Design checkpointing and experiment lineage.
5. Read the RL platform design in real-designs/28-rl-platform.
```

---

## 12. Interview Cheat Sheet

```
If asked “How is RL infrastructure different from normal training?”:

  Normal training uses a fixed dataset.
  RL generates its own data using the current policy.
  That means training, inference, simulation, reward scoring, and evaluation
  must run together in a feedback loop.

If asked “Why is Ray popular for RL?”:

  RL maps naturally to Ray actors:
    - each environment worker is a stateful actor
    - learner is another actor
    - replay buffer can be an actor
    - evaluation workers are actors
  Ray handles scheduling, resources, object store, and cluster execution.

If asked “Why is RLHF expensive?”:

  PPO-style RLHF may require policy, reference, reward, and critic models,
  plus high-throughput generation. For large LLMs this means many GPUs and
  difficult synchronization between inference and training.

If asked “Why are DPO/GRPO popular?”:

  PPO is powerful but operationally complex.
  DPO removes online RL and reward-model serving from the loop.
  GRPO removes the separate value model and uses group-relative rewards.
  Both reduce infrastructure cost and instability.
```

---

## 13. What This Lab Was Missing

```
Already present:
  - Ray distributed execution
  - JAX and PyTorch training frameworks
  - RLHF internals
  - Large-scale RL platform system design

Missing before this folder:
  - A comparison of RL frameworks
  - Environment/simulator ecosystem map
  - Classic RL vs LLM RL distinction
  - Framework selection guide
  - Study path from Gymnasium -> SB3 -> RLlib -> TRL/OpenRLHF/verl
```

Related folders:

```
key-technologies/ml-training/ray/        Ray distributed execution
key-technologies/ml-training/jax/        JAX transformations and TPU training
key-technologies/ml-training/pytorch/    PyTorch training ecosystem
ml-sys/rlhf/                             RLHF algorithm internals
real-designs/28-rl-platform/             Large-scale RL platform design
```
