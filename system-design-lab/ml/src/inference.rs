use crate::fundamentals::softmax;
use rand::Rng;

// =============================================================================
// Inference & Decoding
// =============================================================================

// ── #32 Top-k / Top-p (Nucleus) Sampling ─────────────────────────────────────
// Temperature scaling: logits / temperature (higher T → more random)
// Top-k: keep only the k highest-probability tokens, zero the rest
// Top-p (nucleus): keep the smallest set of tokens whose cumulative prob ≥ p
pub fn sample_top_k_top_p(logits: &[f32], temperature: f32, top_k: usize, top_p: f32) -> usize {
    // Step 1: Temperature scaling
    let scaled: Vec<f32> = logits.iter().map(|&v| v / temperature).collect();

    // Step 2: Sort by logit value (descending)
    let mut indexed: Vec<(usize, f32)> = scaled.iter().cloned().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Step 3: Top-k filtering
    let k = top_k.min(indexed.len());
    let top_k_tokens = &indexed[..k];

    // Step 4: Softmax over remaining tokens
    let top_logits: Vec<f32> = top_k_tokens.iter().map(|(_, v)| *v).collect();
    let probs = softmax(&top_logits);

    // Step 5: Top-p (nucleus) filtering
    let mut cumulative = 0.0;
    let mut cutoff = probs.len();
    for (i, &p) in probs.iter().enumerate() {
        cumulative += p;
        if cumulative >= top_p {
            cutoff = i + 1;
            break;
        }
    }

    // Step 6: Renormalize and sample
    let filtered_probs: Vec<f32> = probs[..cutoff].to_vec();
    let sum: f32 = filtered_probs.iter().sum();
    let normalized: Vec<f32> = filtered_probs.iter().map(|p| p / sum).collect();

    // Weighted random sample
    let mut rng = rand::thread_rng();
    let r: f32 = rng.gen();
    let mut cumul = 0.0;
    for (i, &p) in normalized.iter().enumerate() {
        cumul += p;
        if r < cumul {
            return top_k_tokens[i].0; // return original token index
        }
    }
    top_k_tokens[cutoff - 1].0
}

// ── #33 Beam Search ──────────────────────────────────────────────────────────
// Instead of greedily picking the best token at each step,
// maintain K "beams" (hypotheses) and expand the top-K at each step.
// Returns the highest-scoring complete sequence.
//
// Step 0: ["<start>"]  (1 beam)
// Step 1: ["The",  "A",   "In"]  (top 3)
// Step 2: ["The cat", "The dog", "A cat"]  (expand each, keep top 3)
// ...until <eos> or max_len

#[derive(Clone, Debug)]
struct Beam {
    tokens: Vec<usize>,
    score: f32, // log probability
}

pub fn beam_search(
    // log_prob_fn: given tokens so far, return log probabilities for next token
    log_prob_fn: impl Fn(&[usize]) -> Vec<f32>,
    beam_width: usize,
    max_len: usize,
    eos_token: usize,
    vocab_size: usize,
) -> Vec<usize> {
    let mut beams = vec![Beam { tokens: vec![], score: 0.0 }];
    let mut completed = Vec::new();

    for _step in 0..max_len {
        let mut candidates = Vec::new();

        for beam in &beams {
            let log_probs = log_prob_fn(&beam.tokens);

            for token in 0..vocab_size {
                let new_score = beam.score + log_probs[token];
                let mut new_tokens = beam.tokens.clone();
                new_tokens.push(token);

                if token == eos_token {
                    completed.push(Beam { tokens: new_tokens, score: new_score });
                } else {
                    candidates.push(Beam { tokens: new_tokens, score: new_score });
                }
            }
        }

        // Keep top beam_width candidates
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        beams = candidates.into_iter().take(beam_width).collect();

        if beams.is_empty() { break; }
    }

    // Return best completed beam (or best incomplete if none completed)
    completed.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    if let Some(best) = completed.first() {
        best.tokens.clone()
    } else {
        beams.first().map(|b| b.tokens.clone()).unwrap_or_default()
    }
}

// ── #34 Speculative Decoding ─────────────────────────────────────────────────
// Use a small fast model (draft) to generate N candidate tokens.
// Then verify all N tokens with the large model in ONE forward pass (parallel).
// Accept tokens from draft that match large model's distribution.
// Speedup: draft generates N tokens in time of 1 large-model step.
pub fn speculative_decode(
    // target_log_probs: given tokens, return log probs from LARGE model (batch of N)
    target_log_probs: impl Fn(&[usize], usize) -> Vec<Vec<f32>>,
    // draft_sample: sample 1 token from SMALL model
    draft_sample: impl Fn(&[usize]) -> (usize, f32), // (token, log_prob)
    initial_tokens: &[usize],
    num_draft: usize,   // how many speculative tokens to draft
    num_steps: usize,
) -> Vec<usize> {
    let mut tokens = initial_tokens.to_vec();
    let mut accepted_total = 0;
    let mut drafted_total = 0;

    for _step in 0..num_steps {
        // Draft N tokens with the small model
        let mut draft_tokens = Vec::new();
        let mut draft_probs = Vec::new();
        let mut current = tokens.clone();

        for _ in 0..num_draft {
            let (token, log_prob) = draft_sample(&current);
            draft_tokens.push(token);
            draft_probs.push(log_prob);
            current.push(token);
        }
        drafted_total += num_draft;

        // Verify all draft tokens with the large model (1 forward pass)
        let target_probs = target_log_probs(&tokens, num_draft);

        // Accept/reject each draft token
        let mut rng = rand::thread_rng();
        let mut accepted = 0;
        for i in 0..num_draft {
            let target_prob = target_probs[i][draft_tokens[i]].exp();
            let draft_prob = draft_probs[i].exp();

            // Accept with probability min(1, target_prob / draft_prob)
            let accept_prob = (target_prob / draft_prob).min(1.0);
            if rng.gen::<f32>() < accept_prob {
                tokens.push(draft_tokens[i]);
                accepted += 1;
            } else {
                // Reject: sample from adjusted distribution and stop
                // (simplified: just sample from target)
                let probs: Vec<f32> = target_probs[i].iter().map(|&lp| lp.exp()).collect();
                let sum: f32 = probs.iter().sum();
                let r: f32 = rng.gen::<f32>() * sum;
                let mut cumul = 0.0;
                for (t, &p) in probs.iter().enumerate() {
                    cumul += p;
                    if r < cumul {
                        tokens.push(t);
                        break;
                    }
                }
                break;
            }
        }
        accepted_total += accepted;
    }

    println!("    Speculative: drafted={}, accepted={}, rate={:.0}%",
        drafted_total, accepted_total,
        accepted_total as f32 / drafted_total as f32 * 100.0);
    tokens
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Inference & Decoding ═══\n");

    // Top-k / Top-p sampling
    let logits = vec![2.0, 1.5, 0.5, -1.0, -2.0, 0.1, 0.3, -0.5]; // 8 vocab tokens
    println!("    Logits: {:?}", logits);
    let mut counts = vec![0usize; 8];
    for _ in 0..1000 {
        let token = sample_top_k_top_p(&logits, 1.0, 4, 0.9);
        counts[token] += 1;
    }
    println!("    Top-k=4, Top-p=0.9, T=1.0 (1000 samples):");
    println!("    Counts: {:?}", counts);

    // Beam search
    println!("\n    Beam search (beam=3, vocab=5, max_len=4):");
    let result = beam_search(
        |tokens| {
            // Mock log-prob function: prefer tokens 0 and 1
            let mut probs = vec![-3.0; 5];
            probs[0] = -0.5; // token 0 is likely
            probs[1] = -1.0; // token 1 is somewhat likely
            if tokens.len() >= 3 { probs[4] = -0.1; } // eos likely after 3 tokens
            probs
        },
        3,    // beam width
        6,    // max length
        4,    // eos token = 4
        5,    // vocab size
    );
    println!("    Result: {:?}", result);

    // Speculative decoding
    println!("\n    Speculative decoding (draft 3 tokens per step):");
    let result = speculative_decode(
        |_tokens, n| {
            // Mock target model: uniform-ish log probs
            (0..n).map(|_| {
                vec![-2.0, -1.5, -1.8, -2.2, -3.0]
            }).collect()
        },
        |_tokens| {
            // Mock draft model: biased toward token 1
            let mut rng = rand::thread_rng();
            let token = if rng.gen::<f32>() < 0.6 { 1 } else { rng.gen_range(0..5) };
            (token, -1.5)
        },
        &[0],   // initial tokens
        3,       // draft 3 tokens
        5,       // 5 speculative steps
    );
    println!("    Generated {} tokens\n", result.len() - 1);
}
