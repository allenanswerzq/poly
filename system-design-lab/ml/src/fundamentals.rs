use crate::tensor::Tensor;

// =============================================================================
// Fundamentals — implement from scratch, no framework
// =============================================================================

// ── #1 ReLU ──────────────────────────────────────────────────────────────────
//
//   relu(x) = max(0, x)
//
//   Why it works:
//   - Kills negative values, passes positive values through
//   - Creates sparsity: many neurons output 0 → efficient
//   - No vanishing gradient for positive values (gradient = 1)
//   - Dead neuron problem: if always negative → gradient = 0 → never updates
//
//   Graph:
//        output
//          │    /
//          │   /
//          │  /
//     ─────┼──────── input
//          │
//          │
//
//   Interview: "Why not sigmoid?" → sigmoid squashes to [0,1], gradients vanish
//   for large/small inputs (gradient ≈ 0). ReLU has gradient = 1 for all positive
//   inputs, so deep networks train much faster.
//
pub fn relu(x: &[f32]) -> Vec<f32> {
    x.iter().map(|&v| v.max(0.0)).collect()
}

// ── #19 GELU ─────────────────────────────────────────────────────────────────
//
//   gelu(x) = x · Φ(x) = x · 0.5 · (1 + erf(x / √2))
//
//   Where Φ(x) is the standard normal CDF (probability that a Gaussian
//   random variable is ≤ x).
//
//   Intuition: GELU is a "smooth ReLU". Instead of a hard cutoff at 0,
//   it smoothly transitions. Small negative values get a small (non-zero)
//   output instead of being completely killed.
//
//   Graph:
//        output
//          │     ╱
//          │   ╱    ← looks like ReLU but curved near 0
//          │──╱
//     ─────┼──────── input
//          │
//
//   Used by: GPT, BERT, most modern transformers
//   Why over ReLU? Smoother gradient landscape → slightly better training
//
pub fn gelu(x: &[f32]) -> Vec<f32> {
    x.iter()
        .map(|&v| 0.5 * v * (1.0 + erf(v / std::f32::consts::SQRT_2)))
        .collect()
}

// erf approximation (Abramowitz and Stegun)
fn erf(x: f32) -> f32 {
    let a1 = 0.254_829_6;
    let a2 = -0.284_496_72;
    let a3 = 1.421_413_8;
    let a4 = -1.453_152_1;
    let a5 = 1.061_405_4;
    let p = 0.3275911;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y
}

// ── #2 Softmax ───────────────────────────────────────────────────────────────
//
//   softmax(x_i) = exp(x_i) / Σ exp(x_j)
//
//   Converts raw scores (logits) into probabilities that sum to 1.
//
//   Example: logits = [2.0, 1.0, 0.1]
//     exp([2.0, 1.0, 0.1]) = [7.39, 2.72, 1.11]
//     sum = 11.22
//     softmax = [7.39/11.22, 2.72/11.22, 1.11/11.22] = [0.659, 0.242, 0.099]
//     → Sum = 1.0 ✓
//
//   NUMERICAL STABILITY TRICK (critical for interviews):
//     exp(1000) = Infinity! Overflow.
//     Fix: subtract max before exp.
//     softmax(x_i) = exp(x_i - max(x)) / Σ exp(x_j - max(x))
//     This is mathematically identical but prevents overflow.
//
//     Why? exp(x - max) ≤ exp(0) = 1 → no overflow
//     The max element becomes exp(0) = 1, everything else is < 1.
//
//   Interview: "What happens without the max trick?"
//   → exp(100) = 2.7e43, exp(1000) = Inf. Model outputs garbage.
pub fn softmax(x: &[f32]) -> Vec<f32> {
    let max = x.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = x.iter().map(|&v| (v - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|&e| e / sum).collect()
}

// ── #16 Cross-Entropy Loss ───────────────────────────────────────────────────
//
//   loss = -log(softmax(logits)[target_class])
//
//   Intuition: if the model assigns probability 0.9 to the correct class,
//   loss = -log(0.9) = 0.105 (low). If it assigns 0.01,
//   loss = -log(0.01) = 4.6 (high). Penalizes confident wrong predictions.
//
//   logits = [2.0, 1.0, 0.1], target = class 0
//   softmax = [0.659, 0.242, 0.099]
//   loss = -log(0.659) = 0.417
//
//   LOGSUMEXP TRICK (avoids computing softmax + log separately):
//     log(softmax(x_i)) = x_i - log(Σ exp(x_j))
//     log(Σ exp(x_j)) = max + log(Σ exp(x_j - max))    ← stable version
//
//   This avoids:
//     1. Computing exp (might overflow)
//     2. Computing log of the result (might underflow if prob ≈ 0)
//   Instead we stay in log-space the whole time.
//
//   Interview: "Why not just do -log(softmax(x)[target])?"
//   → Computing softmax then log loses precision. The logsumexp trick
//   is numerically stable and used everywhere (PyTorch's F.cross_entropy).
pub fn cross_entropy_loss(logits: &[f32], target: usize) -> f32 {
    let max = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let logsumexp = max + logits.iter().map(|&v| (v - max).exp()).sum::<f32>().ln();
    -(logits[target] - logsumexp) // -log_softmax[target]
}

// ── #3 Linear Layer ──────────────────────────────────────────────────────────
//
//   y = x @ W^T + b     (the most fundamental neural network operation)
//
//   Shapes:
//     x:      (batch_size, in_features)     e.g., (32, 768) — 32 samples, 768 dims
//     W:      (out_features, in_features)   e.g., (256, 768) — project 768→256
//     W^T:    (in_features, out_features)   e.g., (768, 256)
//     x @ W^T: (batch_size, out_features)   e.g., (32, 256) — output
//     b:      (out_features,)               e.g., (256,) — bias, broadcast-added
//
//   Why W is stored as (out, in) not (in, out)?
//   Convention from PyTorch. Makes indexing by output neuron easy:
//   W[i] = weights for output neuron i.
//
//   Each output neuron computes:
//     y_i = Σ_j (x_j * W[i][j]) + b[i]    ← dot product of input with weights
//
//   This is a learned linear transformation. Without activation functions,
//   stacking linear layers collapses to a single linear layer.
//   That's why we need non-linearities (ReLU, GELU) between them.
pub struct Linear {
    pub weight: Tensor, // (out_features, in_features)
    pub bias: Vec<f32>, // (out_features,)
}

impl Linear {
    // #20 Kaiming Init: std = sqrt(2 / fan_in)
    //
    // Why Kaiming? If weights are too large, activations explode layer by layer.
    // If too small, they vanish. Kaiming init keeps the variance of activations
    // constant across layers (assuming ReLU kills half the values, hence the 2).
    //
    //   std = sqrt(2 / fan_in)
    //   fan_in = number of input features
    //
    //   For a layer with 768 inputs: std = sqrt(2/768) = 0.051
    //   Weights ~ N(0, 0.051²)
    //
    // Without proper init: deep networks (50+ layers) don't train at all.
    // With Kaiming: signal propagates cleanly through hundreds of layers.
    pub fn new(in_features: usize, out_features: usize) -> Self {
        let std = (2.0 / in_features as f32).sqrt();
        Self {
            weight: Tensor::rand_normal(out_features, in_features, std),
            bias: vec![0.0; out_features],
        }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        // x: (batch, in) @ W^T: (in, out) → (batch, out)
        x.matmul(&self.weight.t()).add_bias(&self.bias)
    }
}

// ── #4 LayerNorm ─────────────────────────────────────────────────────────────
//
//   LayerNorm(x) = γ · (x - μ) / √(σ² + ε) + β
//
//   Where:
//     μ = mean of x across features (last dimension)
//     σ² = variance of x across features
//     γ (gamma) = learnable scale, initialized to 1
//     β (beta) = learnable shift, initialized to 0
//     ε = small constant (1e-5) to prevent division by zero
//
//   Example: x = [1, 2, 3, 4]
//     μ = 2.5
//     σ² = 1.25
//     normalized = [-1.34, -0.45, 0.45, 1.34]  (zero mean, unit variance)
//     output = γ · normalized + β                (learnable rescaling)
//
//   Why normalize?
//   Without it, activations drift to very large or small values as they pass
//   through layers, making training unstable. LayerNorm keeps each layer's
//   output in a nice range regardless of the input magnitude.
//
//   LayerNorm vs BatchNorm:
//     LayerNorm: normalize across FEATURES (one sample at a time)
//       → works with any batch size, even batch=1 (inference)
//       → used in Transformers (GPT, BERT, LLaMA)
//     BatchNorm: normalize across BATCH (one feature at a time)
//       → needs batch statistics → problematic for small batches / inference
//       → used in CNNs (ResNet)
pub fn layer_norm(x: &[f32], gamma: &[f32], beta: &[f32], eps: f32) -> Vec<f32> {
    let n = x.len() as f32;
    let mean: f32 = x.iter().sum::<f32>() / n;
    let var: f32 = x.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n;
    let std = (var + eps).sqrt();
    x.iter()
        .enumerate()
        .map(|(i, &v)| ((v - mean) / std) * gamma[i] + beta[i])
        .collect()
}

// ── #7 BatchNorm ─────────────────────────────────────────────────────────────
//
//   BatchNorm(x) = γ · (x - μ_batch) / √(σ²_batch + ε) + β
//
//   Same formula as LayerNorm, but μ and σ² computed across the BATCH dimension.
//
//   Feature j:  batch = [x₁ⱼ, x₂ⱼ, x₃ⱼ, ..., x_Bⱼ]
//               μⱼ = mean of all samples for feature j
//               σ²ⱼ = variance of all samples for feature j
//
//   Train vs Eval:
//     Training: use batch statistics (μ_batch, σ²_batch)
//               also update running statistics with momentum:
//               running_μ = (1-m) · running_μ + m · μ_batch
//     Eval:     use running statistics (accumulated during training)
//               → deterministic output, doesn't depend on other samples in batch
//
//   Why running statistics? At inference, you might have batch_size=1.
//   Computing "batch statistics" of 1 sample is meaningless.
pub struct BatchNorm {
    pub gamma: Vec<f32>,
    pub beta: Vec<f32>,
    pub running_mean: Vec<f32>,
    pub running_var: Vec<f32>,
    pub momentum: f32,
    pub eps: f32,
}

impl BatchNorm {
    pub fn new(features: usize) -> Self {
        Self {
            gamma: vec![1.0; features],
            beta: vec![0.0; features],
            running_mean: vec![0.0; features],
            running_var: vec![1.0; features],
            momentum: 0.1,
            eps: 1e-5,
        }
    }

    // Training mode: normalize using batch statistics, update running stats
    pub fn forward_train(&mut self, x: &Tensor) -> Tensor {
        let (batch, features) = (x.rows, x.cols);
        let mut result = Tensor::zeros(batch, features);

        for f in 0..features {
            // Compute batch mean and var for this feature
            let mut mean = 0.0;
            for b in 0..batch {
                mean += x.get(b, f);
            }
            mean /= batch as f32;

            let mut var = 0.0;
            for b in 0..batch {
                var += (x.get(b, f) - mean).powi(2);
            }
            var /= batch as f32;

            // Normalize
            for b in 0..batch {
                let normed = (x.get(b, f) - mean) / (var + self.eps).sqrt();
                result.set(b, f, normed * self.gamma[f] + self.beta[f]);
            }

            // Update running statistics
            self.running_mean[f] =
                (1.0 - self.momentum) * self.running_mean[f] + self.momentum * mean;
            self.running_var[f] = (1.0 - self.momentum) * self.running_var[f] + self.momentum * var;
        }
        result
    }
}

// ── #8 RMSNorm ───────────────────────────────────────────────────────────────
//
//   RMSNorm(x) = x / RMS(x) · weight
//   where RMS(x) = √(mean(x²) + ε)
//
//   Simpler than LayerNorm: no mean subtraction, no beta.
//   Just divide by the root-mean-square of x, then scale by learnable weight.
//
//   Why LLaMA uses RMSNorm over LayerNorm:
//     1. Faster: no mean computation, no subtraction → ~10% savings
//     2. Empirically: works just as well as LayerNorm for LLMs
//     3. Fewer parameters: no beta (shift) parameter
//
//   LayerNorm: (x - mean) / std * γ + β      (4 operations: mean, sub, div, affine)
//   RMSNorm:   x / RMS(x) * weight            (2 operations: RMS, div+scale)
pub fn rms_norm(x: &[f32], weight: &[f32], eps: f32) -> Vec<f32> {
    let n = x.len() as f32;
    let rms = (x.iter().map(|v| v * v).sum::<f32>() / n + eps).sqrt();
    x.iter()
        .enumerate()
        .map(|(i, &v)| (v / rms) * weight[i])
        .collect()
}

// ── #15 SwiGLU MLP ───────────────────────────────────────────────────────────
//
//   The feed-forward network used in LLaMA, Mistral, and most modern LLMs.
//
//   Standard FFN (GPT-2): GELU(x @ W1) @ W2
//   SwiGLU FFN (LLaMA):   (SiLU(x @ W_gate) ⊙ (x @ W_up)) @ W_down
//
//   SiLU (Swish): silu(x) = x · σ(x) = x · (1 / (1 + exp(-x)))
//   Like a smooth ReLU that allows small negative values through.
//
//   The key insight: GATING. Instead of one projection + activation,
//   SwiGLU uses TWO projections:
//     gate = SiLU(x @ W_gate)    ← "which features to keep"
//     up   = x @ W_up            ← "the actual features"
//     output = gate ⊙ up         ← element-wise product: gate controls flow
//     output = output @ W_down   ← project back to model dimension
//
//   Shape flow (LLaMA-7B example):
//     x:       (seq, 4096)
//     W_gate:  (4096, 11008)  → gate: (seq, 11008)
//     W_up:    (4096, 11008)  → up:   (seq, 11008)
//     gate ⊙ up: (seq, 11008)
//     W_down:  (11008, 4096) → output: (seq, 4096)
//
//   Why 11008? LLaMA uses hidden_dim = 2/3 * 4 * dim, rounded to multiple of 256.
//   SwiGLU has 3 weight matrices instead of 2, so 2/3 factor keeps param count similar.
pub struct SwiGLUMLP {
    pub w_gate: Linear, // (hidden, dim)
    pub w_up: Linear,   // (hidden, dim)
    pub w_down: Linear, // (dim, hidden)
}

impl SwiGLUMLP {
    pub fn new(dim: usize, hidden: usize) -> Self {
        Self {
            w_gate: Linear::new(dim, hidden),
            w_up: Linear::new(dim, hidden),
            w_down: Linear::new(hidden, dim),
        }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let gate = self.w_gate.forward(x);
        let up = self.w_up.forward(x);

        // SiLU(gate) * up
        let mut activated = Tensor::zeros(gate.rows, gate.cols);
        for i in 0..gate.data.len() {
            let silu = gate.data[i] * sigmoid(gate.data[i]);
            activated.data[i] = silu * up.data[i];
        }

        self.w_down.forward(&activated)
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

// ── #17 Dropout ──────────────────────────────────────────────────────────────
//
//   During TRAINING: randomly zero out each element with probability p.
//   During EVAL: do nothing (identity function).
//
//   Why? Prevents co-adaptation: forces each neuron to be useful on its own,
//   not relying on specific other neurons being active. Acts as regularization.
//
//   INVERTED dropout scaling (the standard approach):
//     During training: multiply surviving values by 1/(1-p)
//     During eval: do nothing
//
//   Why scale by 1/(1-p)?
//     Without scaling: training has expected output = (1-p) · x
//                      eval has expected output = x
//                      → mismatch between train and eval!
//     With 1/(1-p):   training expected output = (1-p) · x · 1/(1-p) = x
//                      eval expected output = x
//                      → same expected value in both modes ✓
//
//   Example with p=0.5:
//     Input:  [1.0, 2.0, 3.0, 4.0]
//     Mask:   [1,   0,   1,   0]        ← random 50% zeroed
//     Output: [2.0, 0.0, 6.0, 0.0]     ← surviving values × 2.0
pub fn dropout(x: &[f32], p: f32, training: bool) -> Vec<f32> {
    if !training || p == 0.0 {
        return x.to_vec();
    }
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let scale = 1.0 / (1.0 - p);
    x.iter()
        .map(|&v| if rng.gen::<f32>() < p { 0.0 } else { v * scale })
        .collect()
}

// ── #18 Embedding ────────────────────────────────────────────────────────────
//
//   A lookup table that maps integer token IDs to dense vectors.
//
//   weight: (vocab_size, embed_dim) matrix
//   embedding(token_id) = weight[token_id]   ← just index into the table!
//
//   Example with vocab_size=5, embed_dim=3:
//     weight = [[0.1, 0.2, 0.3],   ← token 0
//               [0.4, 0.5, 0.6],   ← token 1
//               [0.7, 0.8, 0.9],   ← token 2
//               [1.0, 1.1, 1.2],   ← token 3
//               [1.3, 1.4, 1.5]]   ← token 4
//
//     embedding([2, 0, 4]) = [[0.7, 0.8, 0.9],    ← row 2
//                              [0.1, 0.2, 0.3],    ← row 0
//                              [1.3, 1.4, 1.5]]    ← row 4
//
//   That's it. No math. Just weight[index].
//   The magic is that these weights are LEARNED during training —
//   similar tokens end up with similar embedding vectors.
pub struct Embedding {
    pub weight: Tensor, // (vocab_size, embed_dim)
}

impl Embedding {
    pub fn new(vocab_size: usize, embed_dim: usize) -> Self {
        Self {
            weight: Tensor::rand_normal(vocab_size, embed_dim, 0.02),
        }
    }

    pub fn forward(&self, token_ids: &[usize]) -> Tensor {
        let embed_dim = self.weight.cols;
        let mut result = Tensor::zeros(token_ids.len(), embed_dim);
        for (i, &id) in token_ids.iter().enumerate() {
            for j in 0..embed_dim {
                result.set(i, j, self.weight.get(id, j));
            }
        }
        result
    }
}

// ── #21 Gradient Clipping ────────────────────────────────────────────────────
//
//   Problem: gradients can explode (become very large), especially in RNNs
//   and deep transformers. One bad batch → gradient norm = 10000 → weights
//   jump to garbage → training diverges.
//
//   Solution: if ||grad|| > max_norm, scale ALL gradients so ||grad|| = max_norm.
//
//   Algorithm:
//     1. Compute total norm: ||grad|| = sqrt(Σ grad_i²)  across ALL parameters
//     2. If ||grad|| > max_norm:
//        scale = max_norm / ||grad||
//        grad_i *= scale    for every parameter
//
//   KEY: we scale all gradients by the SAME factor. This preserves the
//   direction of the gradient (which parameters to update more/less)
//   while capping the step size.
//
//   Example:
//     grads = [3.0, 4.0, 5.0], max_norm = 1.0
//     ||grad|| = sqrt(9 + 16 + 25) = sqrt(50) ≈ 7.07
//     scale = 1.0 / 7.07 = 0.141
//     clipped = [0.424, 0.566, 0.707]   ||clipped|| = 1.0 ✓
//
//   Typical max_norm: 1.0 for LLMs, 0.5-5.0 depending on model
pub fn clip_grad_norm(grads: &mut [Vec<f32>], max_norm: f32) -> f32 {
    // Compute total norm across all parameter gradients
    let total_norm: f32 = grads
        .iter()
        .flat_map(|g| g.iter())
        .map(|v| v * v)
        .sum::<f32>()
        .sqrt();

    if total_norm > max_norm {
        let scale = max_norm / total_norm;
        for grad in grads.iter_mut() {
            for v in grad.iter_mut() {
                *v *= scale;
            }
        }
    }
    total_norm
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Fundamentals ═══\n");

    // ReLU
    let x = vec![-2.0, -1.0, 0.0, 1.0, 2.0];
    println!("    ReLU({:?}) = {:?}", x, relu(&x));

    // GELU
    println!(
        "    GELU({:?}) = {:?}",
        x,
        gelu(&x)
            .iter()
            .map(|v| format!("{:.3}", v))
            .collect::<Vec<_>>()
    );

    // Softmax
    let logits = vec![2.0, 1.0, 0.1];
    let probs = softmax(&logits);
    println!(
        "    Softmax({:?}) = {:?} (sum={:.3})",
        logits,
        probs
            .iter()
            .map(|v| format!("{:.3}", v))
            .collect::<Vec<_>>(),
        probs.iter().sum::<f32>()
    );

    // Cross-entropy
    let loss = cross_entropy_loss(&logits, 0);
    println!("    CrossEntropy({:?}, target=0) = {:.4}", logits, loss);

    // Linear layer
    let linear = Linear::new(4, 3);
    let x = Tensor::rand(2, 4);
    let y = linear.forward(&x);
    println!("    Linear(in=4, out=3): {} → {}", x.preview(), y.preview());

    // LayerNorm
    let x = vec![1.0, 2.0, 3.0, 4.0];
    let gamma = vec![1.0; 4];
    let beta = vec![0.0; 4];
    let normed = layer_norm(&x, &gamma, &beta, 1e-5);
    println!(
        "    LayerNorm({:?}) = {:?}",
        x,
        normed
            .iter()
            .map(|v| format!("{:.3}", v))
            .collect::<Vec<_>>()
    );

    // RMSNorm
    let weight = vec![1.0; 4];
    let rms = rms_norm(&x, &weight, 1e-5);
    println!(
        "    RMSNorm({:?}) = {:?}",
        x,
        rms.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>()
    );

    // Dropout
    let x = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let dropped = dropout(&x, 0.5, true);
    println!(
        "    Dropout({:?}, p=0.5) = {:?}",
        x,
        dropped
            .iter()
            .map(|v| format!("{:.1}", v))
            .collect::<Vec<_>>()
    );

    // Embedding
    let emb = Embedding::new(100, 8);
    let tokens = emb.forward(&[5, 42, 7]);
    println!("    Embedding([5,42,7]) → {}", tokens.preview());

    // Gradient clipping
    let mut grads = vec![vec![3.0, 4.0], vec![0.0, 5.0]]; // norm = sqrt(50) ≈ 7.07
    let norm = clip_grad_norm(&mut grads, 1.0);
    println!(
        "    GradClip(norm={:.2}, max=1.0) → scaled to norm={:.2}\n",
        norm,
        grads
            .iter()
            .flat_map(|g| g.iter())
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
    );
}
