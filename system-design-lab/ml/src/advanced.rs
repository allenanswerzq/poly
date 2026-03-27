use std::collections::HashMap;

// =============================================================================
// Advanced — Differentiators
// =============================================================================

// ── #35 BPE Tokenizer ────────────────────────────────────────────────────────
// Byte-Pair Encoding: start with characters, iteratively merge the most
// frequent pair into a new token.
//
// Training: "low lower lowest" → characters → find most frequent pair →
//   merge → repeat until vocab_size reached
//
// Encoding: apply merge rules in priority order
pub struct SimpleBPE {
    merges: Vec<(String, String)>, // ordered merge rules
    vocab: HashMap<String, usize>, // token → id
}

impl SimpleBPE {
    pub fn train(text: &str, num_merges: usize) -> Self {
        // Start with character-level tokens
        let mut words: Vec<Vec<String>> = text
            .split_whitespace()
            .map(|w| w.chars().map(|c| c.to_string()).collect())
            .collect();

        let mut merges = Vec::new();

        for _ in 0..num_merges {
            // Count all adjacent pairs
            let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
            for word in &words {
                for pair in word.windows(2) {
                    *pair_counts
                        .entry((pair[0].clone(), pair[1].clone()))
                        .or_default() += 1;
                }
            }

            // Find most frequent pair
            let best = pair_counts.into_iter().max_by_key(|(_, count)| *count);

            let (best_pair, count) = match best {
                Some((pair, count)) if count >= 2 => (pair, count),
                _ => break, // no more pairs to merge
            };

            // Merge this pair everywhere
            let merged = format!("{}{}", best_pair.0, best_pair.1);
            for word in &mut words {
                let mut i = 0;
                while i + 1 < word.len() {
                    if word[i] == best_pair.0 && word[i + 1] == best_pair.1 {
                        word[i] = merged.clone();
                        word.remove(i + 1);
                    } else {
                        i += 1;
                    }
                }
            }

            merges.push(best_pair);
        }

        // Build vocab
        let mut vocab = HashMap::new();
        let mut id = 0;
        // Add all single characters first
        for c in text.chars() {
            let s = c.to_string();
            if s != " " && !vocab.contains_key(&s) {
                vocab.insert(s, id);
                id += 1;
            }
        }
        // Add merged tokens
        for (a, b) in &merges {
            let merged = format!("{}{}", a, b);
            if let std::collections::hash_map::Entry::Vacant(e) = vocab.entry(merged) {
                e.insert(id);
                id += 1;
            }
        }

        Self { merges, vocab }
    }

    pub fn encode(&self, text: &str) -> Vec<String> {
        let mut tokens: Vec<String> = text
            .chars()
            .filter(|c| *c != ' ')
            .map(|c| c.to_string())
            .collect();

        // Apply merge rules in order
        for (a, b) in &self.merges {
            let merged = format!("{}{}", a, b);
            let mut i = 0;
            while i + 1 < tokens.len() {
                if tokens[i] == *a && tokens[i + 1] == *b {
                    tokens[i] = merged.clone();
                    tokens.remove(i + 1);
                } else {
                    i += 1;
                }
            }
        }
        tokens
    }
}

// ── #36 INT8 Quantization ────────────────────────────────────────────────────
// Map FP32 weights to INT8 (256 levels) to reduce memory 4x.
// quantized = round(float / scale) + zero_point
// dequantized = (quantized - zero_point) * scale
pub struct Int8Quantized {
    pub data: Vec<i8>,
    pub scale: f32,
    pub zero_point: i8,
}

impl Int8Quantized {
    // Per-tensor symmetric quantization
    pub fn quantize(float_data: &[f32]) -> Self {
        let abs_max = float_data.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
        let scale = abs_max / 127.0; // map [-abs_max, abs_max] to [-127, 127]
        let zero_point = 0i8; // symmetric quantization

        let data: Vec<i8> = float_data
            .iter()
            .map(|&v| (v / scale).round().clamp(-128.0, 127.0) as i8)
            .collect();

        Self {
            data,
            scale,
            zero_point,
        }
    }

    pub fn dequantize(&self) -> Vec<f32> {
        self.data
            .iter()
            .map(|&v| (v as f32 - self.zero_point as f32) * self.scale)
            .collect()
    }

    pub fn quantization_error(original: &[f32], dequantized: &[f32]) -> f32 {
        original
            .iter()
            .zip(dequantized)
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / original.len() as f32
    }
}

// ── #37 DPO Loss ─────────────────────────────────────────────────────────────
// Direct Preference Optimization: alignment WITHOUT a reward model.
// Loss = -log(σ(β * (log_π(chosen)/log_π_ref(chosen) - log_π(rejected)/log_π_ref(rejected))))
// Simpler than RLHF: just train on (chosen, rejected) pairs directly.
pub fn dpo_loss(
    chosen_logps: &[f32],       // log P(chosen | prompt) under current policy
    rejected_logps: &[f32],     // log P(rejected | prompt) under current policy
    ref_chosen_logps: &[f32],   // log P(chosen | prompt) under reference policy
    ref_rejected_logps: &[f32], // log P(rejected | prompt) under reference policy
    beta: f32,                  // temperature (typically 0.1)
) -> f32 {
    let mut total_loss = 0.0;
    let n = chosen_logps.len();

    for i in 0..n {
        let chosen_reward = beta * (chosen_logps[i] - ref_chosen_logps[i]);
        let rejected_reward = beta * (rejected_logps[i] - ref_rejected_logps[i]);
        // -log(sigmoid(chosen_reward - rejected_reward))
        let logsigmoid = -softplus(-(chosen_reward - rejected_reward));
        total_loss += -logsigmoid;
    }
    total_loss / n as f32
}

// ── #39 PPO Loss ─────────────────────────────────────────────────────────────
// Proximal Policy Optimization: clipped surrogate loss.
// ratio = exp(new_logp - old_logp)
// loss = -min(ratio * advantage, clip(ratio, 1-ε, 1+ε) * advantage)
// The clipping prevents the policy from changing too much in one step.
pub fn ppo_loss(
    new_logps: &[f32],  // log P(action) under new policy
    old_logps: &[f32],  // log P(action) under old policy
    advantages: &[f32], // A(s,a) — how much better than baseline
    clip_ratio: f32,    // ε, typically 0.2
) -> f32 {
    let mut total_loss = 0.0;
    let n = new_logps.len();

    for i in 0..n {
        let ratio = (new_logps[i] - old_logps[i]).exp();
        let unclipped = ratio * advantages[i];
        let clipped = ratio.clamp(1.0 - clip_ratio, 1.0 + clip_ratio) * advantages[i];
        total_loss += -unclipped.min(clipped); // minimize negative = maximize objective
    }
    total_loss / n as f32
}

// ── #38 GRPO Loss ────────────────────────────────────────────────────────────
// Group Relative Policy Optimization: normalize advantages within groups.
// For each prompt, generate G responses, score them, normalize advantages within the group.
pub fn grpo_loss(
    logps: &[f32],       // log P(response) for each response
    rewards: &[f32],     // reward for each response
    group_ids: &[usize], // which group (prompt) each response belongs to
    eps: f32,            // advantage clipping epsilon
) -> f32 {
    // Compute per-group mean and std of rewards
    let mut group_stats: HashMap<usize, (Vec<f32>, f32, f32)> = HashMap::new();
    for (i, &gid) in group_ids.iter().enumerate() {
        group_stats
            .entry(gid)
            .or_insert_with(|| (Vec::new(), 0.0, 0.0))
            .0
            .push(rewards[i]);
    }
    for (_, (rewards, mean, std)) in group_stats.iter_mut() {
        *mean = rewards.iter().sum::<f32>() / rewards.len() as f32;
        *std = (rewards.iter().map(|r| (r - *mean).powi(2)).sum::<f32>() / rewards.len() as f32)
            .sqrt();
        if *std < 1e-6 {
            *std = 1.0;
        }
    }

    let mut total_loss = 0.0;
    let n = logps.len();

    for i in 0..n {
        let (_, mean, std) = &group_stats[&group_ids[i]];
        // Within-group normalized advantage
        let advantage = (rewards[i] - mean) / std;
        total_loss += -logps[i] * advantage.clamp(-1.0 / eps, 1.0 / eps);
    }
    total_loss / n as f32
}

fn softplus(x: f32) -> f32 {
    if x > 20.0 {
        x
    } else {
        (1.0 + x.exp()).ln()
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Advanced ═══\n");

    // BPE Tokenizer
    let text = "low lower lowest low lower";
    let bpe = SimpleBPE::train(text, 10);
    let tokens = bpe.encode("lowest");
    println!("    BPE trained on: \"{}\"", text);
    println!(
        "    Merges: {:?}",
        bpe.merges.iter().take(5).collect::<Vec<_>>()
    );
    println!("    encode(\"lowest\") = {:?}", tokens);
    println!("    Vocab size: {}", bpe.vocab.len());

    // INT8 Quantization
    let floats = vec![0.5, -0.3, 1.2, -0.8, 0.0, 0.7, -1.5, 0.1];
    let quantized = Int8Quantized::quantize(&floats);
    let dequantized = quantized.dequantize();
    let error = Int8Quantized::quantization_error(&floats, &dequantized);
    println!("\n    INT8 quantization:");
    println!("    Original:    {:?}", floats);
    println!("    Quantized:   {:?}", quantized.data);
    println!(
        "    Dequantized: {:?}",
        dequantized
            .iter()
            .map(|v| format!("{:.3}", v))
            .collect::<Vec<_>>()
    );
    println!(
        "    Avg error:   {:.6} (scale={:.6})",
        error, quantized.scale
    );

    // DPO Loss
    let chosen = vec![-1.0, -0.8, -1.2];
    let rejected = vec![-2.0, -1.5, -2.5];
    let ref_chosen = vec![-1.1, -0.9, -1.3];
    let ref_rejected = vec![-2.1, -1.6, -2.6];
    let loss = dpo_loss(&chosen, &rejected, &ref_chosen, &ref_rejected, 0.1);
    println!("\n    DPO loss (β=0.1): {:.4}", loss);

    // PPO Loss
    let new_lp = vec![-1.0, -0.5, -1.5];
    let old_lp = vec![-1.1, -0.6, -1.4];
    let advantages = vec![1.0, -0.5, 0.8];
    let loss = ppo_loss(&new_lp, &old_lp, &advantages, 0.2);
    println!("    PPO loss (ε=0.2): {:.4}", loss);

    // GRPO Loss
    let logps = vec![-1.0, -1.5, -0.8, -2.0, -1.2, -0.9];
    let rewards = vec![0.8, 0.2, 0.9, 0.1, 0.5, 0.7];
    let groups = vec![0, 0, 0, 1, 1, 1]; // 2 prompts, 3 responses each
    let loss = grpo_loss(&logps, &rewards, &groups, 0.2);
    println!("    GRPO loss (2 groups × 3 responses): {:.4}\n", loss);
}
