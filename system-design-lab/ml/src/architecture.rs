use crate::attention::MultiHeadAttention;
use crate::fundamentals::{rms_norm, Linear, SwiGLUMLP};
use crate::tensor::Tensor;

// =============================================================================
// Architecture & Adaptation
// =============================================================================

// ── #13 GPT-2 Block ──────────────────────────────────────────────────────────
// Pre-norm transformer block (used in GPT-2, LLaMA, Mistral):
//   x = x + MHA(LayerNorm(x))
//   x = x + MLP(LayerNorm(x))
pub struct GPT2Block {
    ln1_weight: Vec<f32>,
    mha: MultiHeadAttention,
    ln2_weight: Vec<f32>,
    mlp: SwiGLUMLP,
    d_model: usize,
}

impl GPT2Block {
    pub fn new(d_model: usize, num_heads: usize) -> Self {
        let hidden = d_model * 4; // 4x expansion for MLP
        Self {
            ln1_weight: vec![1.0; d_model],
            mha: MultiHeadAttention::new(d_model, num_heads),
            ln2_weight: vec![1.0; d_model],
            mlp: SwiGLUMLP::new(d_model, hidden),
            d_model,
        }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        // Pre-norm + MHA + residual
        let normed1 = apply_rms_norm_tensor(x, &self.ln1_weight);
        let attn_out = self.mha.forward(&normed1, true); // causal
        let x = x.add(&attn_out); // residual connection

        // Pre-norm + MLP + residual
        let normed2 = apply_rms_norm_tensor(&x, &self.ln2_weight);
        let mlp_out = self.mlp.forward(&normed2);
        x.add(&mlp_out)
    }
}

fn apply_rms_norm_tensor(x: &Tensor, weight: &[f32]) -> Tensor {
    let mut result = Tensor::zeros(x.rows, x.cols);
    for i in 0..x.rows {
        let row = x.row(i);
        let normed = rms_norm(row, weight, 1e-5);
        for j in 0..x.cols {
            result.set(i, j, normed[j]);
        }
    }
    result
}

// ── #26 LoRA (Low-Rank Adaptation) ───────────────────────────────────────────
// Instead of fine-tuning all weights (expensive), add a low-rank update:
//   output = W_frozen @ x + (B @ A) @ x
//   W_frozen: (out, in) — frozen, not updated
//   A: (rank, in)  — tiny, trained
//   B: (out, rank)  — tiny, trained
//   rank << min(in, out), so A and B have very few parameters
//
// Example: W is 4096×4096 = 16M params
//   LoRA rank=16: A is 16×4096 + B is 4096×16 = 131K params (0.8%!)
pub struct LoRALinear {
    frozen_weight: Tensor, // (out, in) — not updated
    lora_a: Tensor,        // (rank, in) — trained
    lora_b: Tensor,        // (out, rank) — trained
    scaling: f32,          // alpha / rank
}

impl LoRALinear {
    pub fn new(in_features: usize, out_features: usize, rank: usize, alpha: f32) -> Self {
        Self {
            frozen_weight: Tensor::rand_normal(out_features, in_features, 0.02),
            lora_a: Tensor::rand_normal(rank, in_features, 0.02),
            lora_b: Tensor::zeros(out_features, rank), // B initialized to zero → LoRA starts as identity
            scaling: alpha / rank as f32,
        }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        // Frozen path: x @ W^T
        let frozen_out = x.matmul(&self.frozen_weight.t());

        // LoRA path: x @ A^T @ B^T * scaling
        let lora_hidden = x.matmul(&self.lora_a.t()); // (batch, rank)
        let lora_out = lora_hidden.matmul(&self.lora_b.t()).scale(self.scaling); // (batch, out)

        frozen_out.add(&lora_out)
    }

    pub fn param_count(&self) -> (usize, usize) {
        let frozen = self.frozen_weight.data.len();
        let lora = self.lora_a.data.len() + self.lora_b.data.len();
        (frozen, lora)
    }
}

// ── #28 Mixture of Experts (MoE) ─────────────────────────────────────────────
// Mixtral-style: instead of 1 MLP, have N expert MLPs.
// A router picks the top-K experts for each token.
// Only K/N of the compute is used per token → more total params, same compute.
pub struct MixtureOfExperts {
    router: Linear,       // (d_model, num_experts) — logits for each expert
    experts: Vec<Linear>, // N expert MLPs (simplified as linear layers)
    num_experts: usize,
    top_k: usize,
}

impl MixtureOfExperts {
    pub fn new(d_model: usize, num_experts: usize, top_k: usize) -> Self {
        let experts = (0..num_experts)
            .map(|_| Linear::new(d_model, d_model))
            .collect();
        Self {
            router: Linear::new(d_model, num_experts),
            experts,
            num_experts,
            top_k,
        }
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let mut result = Tensor::zeros(x.rows, x.cols);

        for row_idx in 0..x.rows {
            // Router: compute expert scores for this token
            let row_tensor = Tensor::from_vec(x.row(row_idx).to_vec(), 1, x.cols);
            let router_logits = self.router.forward(&row_tensor);

            // Softmax over experts
            let logits_slice = &router_logits.data;
            let max = logits_slice
                .iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max);
            let exps: Vec<f32> = logits_slice.iter().map(|&v| (v - max).exp()).collect();
            let sum: f32 = exps.iter().sum();
            let probs: Vec<f32> = exps.iter().map(|e| e / sum).collect();

            // Select top-K experts
            let mut indexed: Vec<(usize, f32)> = probs.iter().cloned().enumerate().collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top_k_experts = &indexed[..self.top_k];

            // Normalize top-k weights to sum to 1
            let top_k_sum: f32 = top_k_experts.iter().map(|(_, w)| w).sum();

            // Run selected experts and combine
            for &(expert_idx, weight) in top_k_experts {
                let expert_out = self.experts[expert_idx].forward(&row_tensor);
                let norm_weight = weight / top_k_sum;
                for j in 0..x.cols {
                    let prev = result.get(row_idx, j);
                    result.set(row_idx, j, prev + norm_weight * expert_out.data[j]);
                }
            }
        }
        result
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Architecture & Adaptation ═══\n");

    // GPT-2 Block
    let block = GPT2Block::new(16, 2);
    let x = Tensor::rand(4, 16); // (seq_len=4, d_model=16)
    let out = block.forward(&x);
    println!(
        "    GPT2Block(d=16, heads=2): {} → {}",
        x.preview(),
        out.preview()
    );

    // LoRA
    let lora = LoRALinear::new(64, 64, 4, 1.0);
    let x = Tensor::rand(2, 64);
    let out = lora.forward(&x);
    let (frozen, trainable) = lora.param_count();
    println!(
        "    LoRA(64→64, rank=4): frozen={}, trainable={} ({:.1}%)",
        frozen,
        trainable,
        trainable as f32 / frozen as f32 * 100.0
    );

    // MoE
    let moe = MixtureOfExperts::new(8, 4, 2); // 4 experts, top-2
    let x = Tensor::rand(3, 8);
    let out = moe.forward(&x);
    println!(
        "    MoE(4 experts, top-2): {} → {}\n",
        x.preview(),
        out.preview()
    );
}
