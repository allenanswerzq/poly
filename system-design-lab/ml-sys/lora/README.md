# LoRA — How Parameter-Efficient Fine-Tuning Works

## What It Is

LoRA (Low-Rank Adaptation) fine-tunes a model by adding TINY trainable matrices while freezing all original weights. Instead of updating 7B parameters, you update ~10M (0.14%).

## The Core Idea

```
Original: y = x @ W            W is (d × d), frozen, huge

LoRA:     y = x @ W + x @ B @ A    B is (d × r), A is (r × d), tiny
                 ↑ frozen           ↑ trainable (r << d)

  W: 4096 × 4096 = 16.7M parameters (frozen)
  A: 16 × 4096   = 65K parameters   (trained)
  B: 4096 × 16   = 65K parameters   (trained)
  Total LoRA:     = 131K parameters  (0.8% of W!)

  rank r = 16 is typical. Lower rank = fewer params, less expressiveness.

Why it works:
  The weight updates during fine-tuning are LOW-RANK.
  A full (d × d) update matrix has d² degrees of freedom,
  but the useful updates live in a low-dimensional subspace.
  LoRA directly parameterizes this subspace with rank r.
```

## Initialization — Why B is Zero

```
A: initialized with random Gaussian (like Kaiming)
B: initialized to ALL ZEROS

Why? At the start of fine-tuning:
  LoRA output = x @ B @ A = x @ 0 @ A = 0

  The model starts as the original pre-trained model.
  LoRA contribution grows gradually during training.
  No sudden jump in behavior when you "attach" LoRA.
```

## Where to Apply LoRA

```
A transformer has these weight matrices:
  Attention: W_q, W_k, W_v, W_o     (4 matrices per layer)
  MLP:       W_gate, W_up, W_down   (3 matrices per layer)

Which to LoRA:
  Minimum: W_q, W_v only            (original paper)
  Better:  W_q, W_k, W_v, W_o       (attention only)
  Best:    all 7 matrices per layer  (QLoRA paper finding)

LLaMA-7B, rank=16, all matrices:
  Original params: 6.7B
  LoRA params:     ~10M (0.15%)
  Training memory:  save ~70% (only LoRA grads + optimizer states)
```

## QLoRA — LoRA + Quantization

```
Problem: even with LoRA, the frozen weights take 14GB (7B × 2 bytes fp16).
QLoRA: quantize frozen weights to 4-bit (NF4 format).

  Frozen weights: 7B × 0.5 bytes = 3.5 GB (4-bit!)
  LoRA weights:   10M × 2 bytes  = 20 MB (fp16)
  Optimizer:      10M × 8 bytes  = 80 MB (Adam states for LoRA only)
  ─────────────────────────────────────────
  Total: ~4 GB → Fine-tune a 7B model on a single consumer GPU!

How NF4 (Normal Float 4-bit) quantization works:
  4 bits = 16 possible values
  NF4: the 16 values are placed at quantiles of a normal distribution.
  Since neural network weights are approximately Gaussian,
  NF4 minimizes quantization error for typical weight distributions.

  + double quantization: quantize the quantization scales too → save ~0.4 bits/param

Result: fine-tune LLaMA-65B on a single 48GB GPU.
Quality: 97-99% of full fine-tuning quality.
```

## Merging LoRA — Deployment

```
After training, LoRA can be MERGED into the base model:

  W_merged = W + B @ A × (alpha / rank)

  alpha: a scaling hyperparameter (typically equal to rank)

  After merging:
    - No extra inference cost (just a regular model)
    - No extra memory (LoRA weights absorbed into W)
    - Can serve with vLLM/TGI normally

Or keep LoRA separate:
  - Switch between different tasks by swapping LoRA adapters
  - Same base model + different LoRA for different use cases
  - vLLM supports serving multiple LoRA adapters simultaneously
```

## LoRA Variants

```
LoRA:     y = Wx + BAx                     (basic low-rank)
LoRA+:    y = Wx + BAx, different lr for A and B  (better optimization)
DoRA:     decompose W into magnitude and direction, LoRA on direction
rsLoRA:   scale by 1/sqrt(r) instead of 1/r (better for high rank)
AdaLoRA:  dynamically adjust rank during training (important layers get higher rank)
VeRA:     share A,B across layers, only train scaling vectors (even fewer params)

In practice: basic LoRA with rank=16-64 works well for most tasks.
```
