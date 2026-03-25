# ML Interview Problem Set

All problems implemented from scratch in Rust — no framework, pure math.
Each module demonstrates the core algorithm with clear comments.

Run: `cargo run -p ml-problems`

## Problem List

### 🧱 Fundamentals — "Implement X from scratch"

| # | Problem | Difficulty | Freq | Key Concepts |
|---|---------|-----------|------|-------------|
| 1 | ReLU | Easy | 🔥 | Activation functions, element-wise ops |
| 2 | Softmax | Easy | 🔥 | Numerical stability, exp/log tricks |
| 3 | Linear Layer | Medium | 🔥 | y = xW^T + b, Kaiming init |
| 4 | LayerNorm | Medium | 🔥 | Normalization, affine transform |
| 7 | BatchNorm | Medium | ⭐ | Batch vs layer statistics |
| 8 | RMSNorm | Medium | ⭐ | LLaMA-style norm |
| 15 | SwiGLU MLP | Medium | ⭐ | Gated FFN, SiLU(gate) * up |
| 16 | Cross-Entropy Loss | Easy | 🔥 | Log-softmax, logsumexp trick |
| 17 | Dropout | Easy | 🔥 | Train/eval mode, inverted scaling |
| 18 | Embedding | Easy | 🔥 | Lookup table |
| 19 | GELU | Easy | ⭐ | Gaussian error linear unit |
| 20 | Kaiming Init | Easy | ⭐ | std = sqrt(2/fan_in) |
| 21 | Gradient Clipping | Easy | ⭐ | Norm-based clipping |
| 22 | Conv2d | Medium | 🔥 | Convolution, stride/padding |
| 40 | Linear Regression | Medium | 🔥 | Normal equation, GD |

### 🧠 Attention Mechanisms

| # | Problem | Difficulty | Freq | Key Concepts |
|---|---------|-----------|------|-------------|
| 5 | Scaled Dot-Product Attention | Hard | 🔥 | softmax(QK^T/√d_k)V |
| 6 | Multi-Head Attention | Hard | 🔥 | Split/concat, projection |
| 9 | Causal Self-Attention | Hard | 🔥 | Autoregressive masking with -inf |
| 10 | Grouped Query Attention | Hard | ⭐ | KV sharing across heads |
| 11 | Sliding Window Attention | Hard | ⭐ | Local attention, O(n·w) |
| 12 | Linear Attention | Hard | 💡 | Kernel trick, O(n·d²) |
| 14 | KV Cache Attention | Hard | 🔥 | Incremental decoding |
| 23 | Cross-Attention | Medium | ⭐ | Q from decoder, K/V from encoder |
| 24 | RoPE | Hard | 🔥 | Rotary position embedding |
| 25 | Flash Attention | Hard | 💡 | Tiled attention, online softmax |

### 🏗️ Architecture & Adaptation

| # | Problem | Difficulty | Freq | Key Concepts |
|---|---------|-----------|------|-------------|
| 13 | GPT-2 Block | Hard | ⭐ | Pre-norm, causal MHA + MLP |
| 26 | LoRA | Medium | ⭐ | Low-rank adaptation |
| 27 | ViT Patch Embedding | Medium | 💡 | Image → patches → projection |
| 28 | Mixture of Experts | Hard | ⭐ | Top-k routing, expert MLPs |

### ⚙️ Training & Optimization

| # | Problem | Difficulty | Freq | Key Concepts |
|---|---------|-----------|------|-------------|
| 29 | Adam Optimizer | Medium | ⭐ | Momentum + RMSProp, bias correction |
| 30 | Cosine LR Scheduler | Medium | ⭐ | Linear warmup + cosine annealing |
| 31 | Gradient Accumulation | Easy | 💡 | Micro-batching, loss scaling |

### 🎯 Inference & Decoding

| # | Problem | Difficulty | Freq | Key Concepts |
|---|---------|-----------|------|-------------|
| 32 | Top-k / Top-p Sampling | Medium | 🔥 | Nucleus sampling, temperature |
| 33 | Beam Search | Medium | 🔥 | Hypothesis expansion, pruning |
| 34 | Speculative Decoding | Hard | 💡 | Accept/reject, draft model |

### 🔬 Advanced

| # | Problem | Difficulty | Freq | Key Concepts |
|---|---------|-----------|------|-------------|
| 35 | BPE Tokenizer | Hard | 💡 | Byte-pair encoding, merge rules |
| 36 | INT8 Quantization | Hard | 💡 | Per-channel quantize, scale/zero-point |
| 37 | DPO Loss | Hard | 💡 | Direct preference optimization |
| 38 | GRPO Loss | Hard | 💡 | Group relative policy optimization |
| 39 | PPO Loss | Hard | 💡 | Clipped surrogate loss |

## Source Structure

```
src/
├── main.rs            — runs all demos
├── tensor.rs          — minimal 2D tensor type (Vec<f32> based)
├── fundamentals.rs    — ReLU, softmax, norms, linear, losses, activations
├── attention.rs       — all attention variants + RoPE + KV cache
├── architecture.rs    — GPT-2 block, LoRA, MoE
├── training.rs        — Adam, cosine LR, gradient clipping
├── inference.rs       — top-k/p sampling, beam search, speculative decoding
└── advanced.rs        — BPE tokenizer, quantization, DPO/PPO/GRPO loss
```
