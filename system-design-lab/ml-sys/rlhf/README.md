# RLHF — How Alignment Training Works Under the Hood

## What It Is

RLHF (Reinforcement Learning from Human Feedback) takes a pre-trained LLM and aligns it to human preferences — making it helpful, harmless, and honest. It's what turns GPT → ChatGPT.

## The Three Stages

```
Stage 1: Supervised Fine-Tuning (SFT)
  Train on (prompt, ideal_response) pairs written by humans.
  Teaches the model the FORMAT of helpful responses.
  Standard supervised training (cross-entropy loss on response tokens).

Stage 2: Reward Model Training
  Collect: human preferences (response A better than response B for prompt P).
  Train a reward model: given (prompt, response) → scalar score.
  The reward model learns WHAT humans prefer without explicit rules.

Stage 3: RL Optimization (PPO or alternatives)
  Use the reward model to optimize the policy (the LLM):
    Generate responses → score with reward model → update LLM to get higher scores
  With a constraint: don't drift too far from the SFT model (KL penalty).
```

## Stage 2: Reward Model — The Details

```
Training data: pairs of (prompt, chosen_response, rejected_response)
  Prompt: "Explain quantum computing"
  Chosen:   "Quantum computing uses qubits..." (clear, accurate)
  Rejected: "It's basically magic computers..." (vague, wrong)

Loss function (Bradley-Terry model):
  loss = -log(σ(r(prompt, chosen) - r(prompt, rejected)))

  Where:
    r(prompt, response) = reward model output (scalar)
    σ = sigmoid function

  Intuition: maximize the gap between chosen and rejected scores.
  If chosen score is much higher → loss is low.
  If scores are close or reversed → loss is high, push them apart.

Architecture:
  Take the SFT model, replace the output head with a scalar head:
    LLM backbone → last hidden state → linear(hidden_dim, 1) → scalar reward
  The backbone understands language; the new head learns to score quality.
```

## Stage 3: PPO — How the RL Loop Works

```
Four models running simultaneously:

  1. Policy (Actor): the LLM being trained. Generates responses.
  2. Reference: frozen copy of the SFT model. Prevents drift.
  3. Reward Model: scores responses. Frozen.
  4. Value Critic: estimates expected reward. Trained alongside policy.

One PPO iteration:
  ┌────────────────────────────────────────────────────────┐
  │ 1. Sample prompts from dataset                         │
  │ 2. Generate responses using Policy model (inference)   │
  │ 3. Score responses with Reward model                   │
  │ 4. Compute advantages:                                 │
  │      advantage = reward - value_estimate                │
  │      (how much better than expected)                    │
  │ 5. PPO loss:                                           │
  │      ratio = π_new(action) / π_old(action)             │
  │      loss = -min(ratio × advantage,                    │
  │               clip(ratio, 1-ε, 1+ε) × advantage)      │
  │ 6. KL penalty: β × KL(π_new || π_ref)                 │
  │      (don't drift too far from reference model)        │
  │ 7. Update Policy and Critic with gradient descent      │
  └────────────────────────────────────────────────────────┘

The clipping in step 5:
  If advantage > 0 (action was good):
    ratio > 1+ε → capped (don't over-exploit this good action)
  If advantage < 0 (action was bad):
    ratio < 1-ε → capped (don't over-penalize)
  This prevents large, destabilizing policy updates.
```

## Alternatives to PPO

```
DPO (Direct Preference Optimization):
  Skip the reward model entirely!
  Train directly on preference pairs using a modified loss:
    loss = -log σ(β(log π(chosen)/π_ref(chosen) - log π(rejected)/π_ref(rejected)))

  Insight: the optimal RLHF policy can be expressed in closed form.
  The reward model is implicitly defined by the policy itself.

  Pros: simpler (no RL, no reward model, no value function, no PPO)
  Cons: less flexible (can't shape rewards, no online generation)

GRPO (Group Relative Policy Optimization):
  Generate G responses per prompt, use rewards to compute within-group advantages.
  No value model needed (advantages computed from group statistics).
  Used by: DeepSeek

KTO (Kahneman-Tversky Optimization):
  Only needs binary feedback (good/bad) not pairwise preferences.
  Based on Prospect Theory (loss aversion).

Comparison:
  Method   Reward model?  Value model?  Online gen?  Complexity
  PPO      yes            yes           yes          highest
  DPO      no             no            no           lowest
  GRPO     no (uses raw)  no            yes          medium
  KTO      no             no            no           low
```

## Infrastructure Cost

```
RLHF for a 70B model (one training run):

  Policy generation (vLLM):     64 × A100 (inference mode)
  Reference model:              64 × A100 (inference mode)
  Reward model:                 16 × A100 (inference mode)
  Critic model:                 64 × A100 (training mode)
  Policy training (FSDP):       64 × A100 (training mode)
  ─────────────────────────────────────────────────────
  Theoretical total:           272 × A100

  In practice with overlapping: ~144 × A100
  Cost: ~$400/hour × 30 hours = ~$12,000 per RLHF run

  DPO alternative: only need Policy + Reference = ~64 × A100
  Cost: ~$100/hour × 10 hours = ~$1,000 per DPO run (12x cheaper!)

  This is why DPO/GRPO are increasingly preferred over PPO.
```
