use crate::tensor::Tensor;

// =============================================================================
// Attention Mechanisms — the heart of modern ML
// =============================================================================

// ── #5 Scaled Dot-Product Attention ──────────────────────────────────────────
//
//   Attention(Q, K, V) = softmax(Q · K^T / √d_k) · V
//
//   The most important equation in modern ML. Here's what each part does:
//
//   Q (Query):  "What am I looking for?"       shape: (seq_len, d_k)
//   K (Key):    "What do I contain?"            shape: (seq_len, d_k)
//   V (Value):  "What information do I carry?"  shape: (seq_len, d_v)
//
//   Step 1: Q · K^T → (seq_len, seq_len) — the attention matrix
//     Each element [i][j] = dot product of query_i and key_j
//     = "how much should token i pay attention to token j?"
//
//     Example (4 tokens):
//       scores = Q @ K^T = [[0.8, 0.2, 0.1, 0.0],   ← token 0 attends mostly to itself
//                            [0.1, 0.7, 0.3, 0.1],   ← token 1 attends mostly to itself
//                            [0.0, 0.5, 0.2, 0.9],   ← token 2 attends to tokens 1 and 3
//                            [0.1, 0.1, 0.8, 0.5]]   ← token 3 attends to token 2
//
//   Step 2: / √d_k — scale down scores
//     Without scaling: dot products grow with d_k → softmax becomes very peaked
//     (one score dominates, others → 0). With scaling: balanced attention.
//     Why √d_k? Dot product variance ∝ d_k, so dividing by √d_k → variance ≈ 1.
//
//   Step 3: softmax → attention weights (each row sums to 1)
//     Converts raw scores to probabilities.
//
//   Step 4: weights @ V → output
//     Each token's output = weighted sum of all value vectors.
//     Token i attends to token j with weight w_ij → pulls in v_j proportionally.
//
//     output_i = Σ_j w_ij · v_j    (weighted average of values)
pub fn scaled_dot_product_attention(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    mask: Option<&Tensor>,
) -> Tensor {
    let d_k = q.cols as f32;
    let scale = 1.0 / d_k.sqrt();

    // Q @ K^T → (seq_q, seq_k)
    let mut scores = q.matmul(&k.t()).scale(scale);

    // Apply mask (e.g., causal mask: -inf for future tokens)
    if let Some(mask) = mask {
        for i in 0..scores.data.len() {
            if mask.data[i] == 0.0 {
                scores.data[i] = f32::NEG_INFINITY;
            }
        }
    }

    // Softmax per row
    let mut weights = Tensor::zeros(scores.rows, scores.cols);
    for i in 0..scores.rows {
        let row = &scores.data[i * scores.cols..(i + 1) * scores.cols];
        let max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = row.iter().map(|&v| (v - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        for j in 0..scores.cols {
            weights.set(i, j, exps[j] / sum);
        }
    }

    // weights @ V → (seq_q, d_v)
    weights.matmul(v)
}

// ── #9 Causal Self-Attention ─────────────────────────────────────────────────
//
//   Same as scaled dot-product attention but with a MASK:
//   Token i can only attend to tokens 0, 1, ..., i (NOT future tokens).
//
//   This is AUTOREGRESSIVE: when generating text, token 5 shouldn't see
//   token 6 because token 6 hasn't been generated yet.
//
//   The mask is a lower-triangular matrix:
//     [[1, 0, 0, 0],     token 0: sees only itself
//      [1, 1, 0, 0],     token 1: sees tokens 0, 1
//      [1, 1, 1, 0],     token 2: sees tokens 0, 1, 2
//      [1, 1, 1, 1]]     token 3: sees all previous tokens
//
//   Where mask = 0, we set the score to -infinity BEFORE softmax.
//   softmax(-inf) = 0, so those positions get zero attention weight.
//
//   Used in: GPT, LLaMA, all decoder-only language models.
pub fn causal_mask(seq_len: usize) -> Tensor {
    let mut mask = Tensor::zeros(seq_len, seq_len);
    for i in 0..seq_len {
        for j in 0..=i {
            mask.set(i, j, 1.0); // 1 = allowed, 0 = masked (-inf)
        }
    }
    mask
}

pub fn causal_self_attention(q: &Tensor, k: &Tensor, v: &Tensor) -> Tensor {
    let mask = causal_mask(q.rows);
    scaled_dot_product_attention(q, k, v, Some(&mask))
}

// ── #6 Multi-Head Attention ──────────────────────────────────────────────────
//
//   Instead of one big attention, run H smaller attentions in parallel,
//   each looking at different "aspects" of the input.
//
//   Think of it like this:
//     Head 1 might learn to attend to syntactic relationships (subject-verb)
//     Head 2 might learn to attend to semantic similarity
//     Head 3 might learn to attend to positional proximity
//     ... each head learns a different "type" of attention
//
//   Algorithm:
//     1. Project: Q,K,V = x @ W_q, x @ W_k, x @ W_v   (each is d_model wide)
//     2. Split into H heads: Q_h = Q[:, h*d_head : (h+1)*d_head]
//        d_head = d_model / num_heads (e.g., 768/12 = 64)
//     3. Each head: Attention(Q_h, K_h, V_h) → (seq_len, d_head)
//     4. Concat all heads: → (seq_len, d_model)
//     5. Output projection: concat @ W_o → (seq_len, d_model)
//
//   Shape flow (GPT-2 small: d_model=768, 12 heads):
//     x:          (seq_len, 768)
//     Q,K,V:      (seq_len, 768)  — linear projection
//     per head:   (seq_len, 64)   — 768/12 = 64 per head
//     attention:  (seq_len, 64)   — per head output
//     concat:     (seq_len, 768)  — all 12 heads joined
//     output:     (seq_len, 768)  — final projection
//
//   Total params: 4 × d_model² = 4 × 768² = 2.4M (W_q, W_k, W_v, W_o)

pub struct MultiHeadAttention {
    num_heads: usize,
    d_head: usize,
    w_q: Tensor, // (d_model, d_model)
    w_k: Tensor,
    w_v: Tensor,
    w_o: Tensor,
}

impl MultiHeadAttention {
    pub fn new(d_model: usize, num_heads: usize) -> Self {
        let d_head = d_model / num_heads;
        let std = (1.0 / d_model as f32).sqrt();
        Self {
            num_heads,
            d_head,
            w_q: Tensor::rand_normal(d_model, d_model, std),
            w_k: Tensor::rand_normal(d_model, d_model, std),
            w_v: Tensor::rand_normal(d_model, d_model, std),
            w_o: Tensor::rand_normal(d_model, d_model, std),
        }
    }

    pub fn forward(&self, x: &Tensor, causal: bool) -> Tensor {
        let seq_len = x.rows;
        let d_model = x.cols;

        // Project Q, K, V
        let q = x.matmul(&self.w_q.t());
        let k = x.matmul(&self.w_k.t());
        let v = x.matmul(&self.w_v.t());

        // For simplicity, we process heads sequentially and concat
        // In real code, this is reshaped + batched matmul
        let mask = if causal {
            Some(causal_mask(seq_len))
        } else {
            None
        };
        let mut all_heads = Vec::new();

        for h in 0..self.num_heads {
            let start = h * self.d_head;
            let end = start + self.d_head;

            // Extract head h from Q, K, V
            let q_h = extract_cols(&q, start, end);
            let k_h = extract_cols(&k, start, end);
            let v_h = extract_cols(&v, start, end);

            let attn = scaled_dot_product_attention(&q_h, &k_h, &v_h, mask.as_ref());
            all_heads.push(attn);
        }

        // Concat heads → (seq_len, d_model)
        let concat = concat_cols(&all_heads);

        // Output projection
        concat.matmul(&self.w_o.t())
    }
}

// ── #10 Grouped Query Attention (GQA) ────────────────────────────────────────
//
//   The KV cache is the memory bottleneck for LLM inference.
//   Each head needs its own K and V cache → lots of memory.
//
//   GQA saves memory by SHARING K,V heads across multiple Q heads:
//
//   Multi-Head (MHA):   32 Q heads, 32 K heads, 32 V heads  (standard)
//   Grouped Query (GQA): 32 Q heads,  8 K heads,  8 V heads  (LLaMA 2/3)
//   Multi-Query (MQA):  32 Q heads,  1 K head,   1 V head   (extreme)
//
//   group_size = num_q_heads / num_kv_heads = 32/8 = 4
//   Q heads 0,1,2,3 share KV head 0
//   Q heads 4,5,6,7 share KV head 1
//   ...etc
//
//   Memory savings: KV cache is 4x smaller with GQA(32q,8kv) vs MHA(32q,32kv)
//   Quality: barely any degradation vs full MHA (validated by LLaMA 2 paper)
pub fn grouped_query_attention(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    num_q_heads: usize,
    num_kv_heads: usize,
    d_head: usize,
    causal: bool,
) -> Tensor {
    let seq_len = q.rows;
    let group_size = num_q_heads / num_kv_heads; // e.g., 32/8 = 4 Q heads per KV head
    let mask = if causal {
        Some(causal_mask(seq_len))
    } else {
        None
    };

    let mut all_heads = Vec::new();

    for qh in 0..num_q_heads {
        let kv_idx = qh / group_size; // which KV head this Q head uses

        let q_h = extract_cols(q, qh * d_head, (qh + 1) * d_head);
        let k_h = extract_cols(k, kv_idx * d_head, (kv_idx + 1) * d_head);
        let v_h = extract_cols(v, kv_idx * d_head, (kv_idx + 1) * d_head);

        let attn = scaled_dot_product_attention(&q_h, &k_h, &v_h, mask.as_ref());
        all_heads.push(attn);
    }

    concat_cols(&all_heads)
}

// ── #11 Sliding Window Attention ─────────────────────────────────────────────
// Mistral-style: each token only attends to the last W tokens.
// O(n * w) instead of O(n²). Good for long sequences.
pub fn sliding_window_mask(seq_len: usize, window_size: usize) -> Tensor {
    let mut mask = Tensor::zeros(seq_len, seq_len);
    for i in 0..seq_len {
        let start = if i >= window_size {
            i - window_size + 1
        } else {
            0
        };
        for j in start..=i {
            mask.set(i, j, 1.0);
        }
    }
    mask
}

// ── #12 Linear Attention ─────────────────────────────────────────────────────
// Instead of softmax(QK^T)V which is O(n²d),
// use φ(Q)(φ(K)^T V) which is O(nd²) — linear in sequence length!
// φ is a feature map (here: ELU + 1 for positivity).
pub fn linear_attention(q: &Tensor, k: &Tensor, v: &Tensor) -> Tensor {
    // φ(x) = elu(x) + 1 (ensures non-negative)
    let phi_q = elu_plus_one(q);
    let phi_k = elu_plus_one(k);

    // KV = φ(K)^T @ V → (d_k, d_v) — computed ONCE
    let kv = phi_k.t().matmul(v);
    // Z = φ(K)^T @ 1 → (d_k, 1) — normalization
    let ones = Tensor::from_vec(vec![1.0; v.rows], v.rows, 1);
    let z = phi_k.t().matmul(&ones);

    // output_i = (φ(Q_i) @ KV) / (φ(Q_i) @ Z)
    let numerator = phi_q.matmul(&kv);
    let denominator = phi_q.matmul(&z);

    let mut result = Tensor::zeros(numerator.rows, numerator.cols);
    for i in 0..numerator.rows {
        for j in 0..numerator.cols {
            result.set(i, j, numerator.get(i, j) / (denominator.get(i, 0) + 1e-6));
        }
    }
    result
}

fn elu_plus_one(x: &Tensor) -> Tensor {
    let data: Vec<f32> = x
        .data
        .iter()
        .map(|&v| if v >= 0.0 { v + 1.0 } else { v.exp() })
        .collect();
    Tensor::from_vec(data, x.rows, x.cols)
}

// ── #14 KV Cache Attention ───────────────────────────────────────────────────
//
//   During autoregressive decoding (generating one token at a time):
//
//   Without KV cache:
//     Step 1: process "The"        → compute K,V for [The]
//     Step 2: process "The cat"    → recompute K,V for [The, cat]  ← WASTED!
//     Step 3: process "The cat is" → recompute K,V for [The, cat, is]  ← WASTED!
//     Each step recomputes ALL previous tokens. O(n²) total.
//
//   With KV cache:
//     Step 1: process "The"        → cache K,V for [The]
//     Step 2: process "cat" only   → compute new K,V, APPEND to cache
//             attend: new Q (1 token) × cached K,V (2 tokens)
//     Step 3: process "is" only    → append to cache
//             attend: new Q (1 token) × cached K,V (3 tokens)
//     Each step only processes 1 new token. O(n) per step, O(n²) total.
//
//   The cache grows linearly: cache_size = seq_len × num_layers × num_heads × d_head × 2
//   For LLaMA-70B at 4096 context: ~40GB of KV cache!
//   This is why PagedAttention (vLLM) matters — manages this memory efficiently.
pub struct KVCache {
    pub k_cache: Vec<Vec<f32>>, // cached key vectors
    pub v_cache: Vec<Vec<f32>>, // cached value vectors
    pub d_head: usize,
}

impl KVCache {
    pub fn new(d_head: usize) -> Self {
        Self {
            k_cache: Vec::new(),
            v_cache: Vec::new(),
            d_head,
        }
    }

    // Append new K,V and compute attention for the new query
    pub fn attend(&mut self, q: &[f32], k: &[f32], v: &[f32]) -> Vec<f32> {
        self.k_cache.push(k.to_vec());
        self.v_cache.push(v.to_vec());

        let seq_len = self.k_cache.len();
        let d_k = self.d_head as f32;
        let scale = 1.0 / d_k.sqrt();

        // Compute scores: q @ each cached k
        let scores: Vec<f32> = self
            .k_cache
            .iter()
            .map(|ki| q.iter().zip(ki).map(|(a, b)| a * b).sum::<f32>() * scale)
            .collect();

        // Softmax
        let max = scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = scores.iter().map(|&s| (s - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let weights: Vec<f32> = exps.iter().map(|&e| e / sum).collect();

        // Weighted sum of cached values
        let mut output = vec![0.0; self.d_head];
        for (i, w) in weights.iter().enumerate() {
            for (j, v) in self.v_cache[i].iter().enumerate() {
                output[j] += w * v;
            }
        }
        output
    }
}

// ── #24 RoPE (Rotary Position Embedding) ─────────────────────────────────────
//
//   The position encoding used by LLaMA, Mistral, and most modern LLMs.
//
//   Problem: attention is permutation-invariant (doesn't know token order).
//   Solution: encode position by ROTATING Q and K vectors in 2D planes.
//
//   For each pair of dimensions (2i, 2i+1), rotate by angle = position × θ_i
//   where θ_i = 1 / 10000^(2i/d)
//
//   Rotation matrix (for 1 pair of dimensions):
//     [cos(pθ)  -sin(pθ)] [q_2i  ]   [q_2i·cos(pθ) - q_{2i+1}·sin(pθ)]
//     [sin(pθ)   cos(pθ)] [q_{2i+1}] = [q_2i·sin(pθ) + q_{2i+1}·cos(pθ)]
//
//   Why it works: the DOT PRODUCT between rotated Q and K depends on their
//   RELATIVE position (p_q - p_k), not absolute positions.
//   This gives the model a natural sense of "how far apart are these tokens?"
//
//   Why rotation? It's a length-preserving transformation:
//   ||rotated(q)|| = ||q||. The information content doesn't change,
//   only the "direction" encodes position.
//
//   θ values decrease with dimension pair index:
//     Pair 0: θ = 1.0000    → rotates fast (encodes fine-grained position)
//     Pair 1: θ = 0.0100    → rotates slower
//     Pair 2: θ = 0.0001    → rotates very slowly (encodes broad position)
//   This multi-frequency encoding is similar to Fourier features.
pub fn apply_rope(x: &Tensor, start_pos: usize) -> Tensor {
    let (seq_len, d) = (x.rows, x.cols);
    let mut result = x.clone();

    for pos in 0..seq_len {
        let abs_pos = start_pos + pos;
        for i in (0..d).step_by(2) {
            let dim_pair = i / 2;
            let theta = 1.0 / (10000.0_f32).powf(2.0 * dim_pair as f32 / d as f32);
            let angle = abs_pos as f32 * theta;
            let (cos_a, sin_a) = (angle.cos(), angle.sin());

            let x0 = x.get(pos, i);
            let x1 = x.get(pos, i + 1);
            result.set(pos, i, x0 * cos_a - x1 * sin_a);
            result.set(pos, i + 1, x0 * sin_a + x1 * cos_a);
        }
    }
    result
}

// ── #25 Flash Attention (simplified) ─────────────────────────────────────────
//
//   Standard attention materializes the full N×N attention matrix:
//     scores = Q @ K^T → N×N matrix → stored in GPU HBM → slow!
//     For N=8192, d=128: attention matrix = 8192² × 4 bytes = 256MB per head!
//
//   Flash Attention NEVER stores the full N×N matrix.
//   Instead, it processes in BLOCKS and uses ONLINE SOFTMAX:
//
//   Algorithm:
//     For each Q block:
//       For each K,V block:
//         1. Compute partial scores (Q_block @ K_block^T)
//         2. Update running softmax (track max and sum)
//         3. Accumulate weighted V contributions
//       Normalize at the end
//
//   Online softmax trick:
//     Normal softmax: need ALL scores to compute max and sum → 2 passes
//     Online softmax: maintain running max and running sum → 1 pass
//     When a new score exceeds the running max:
//       rescale all previous accumulations by exp(old_max - new_max)
//
//   Memory: O(N) instead of O(N²) — the attention matrix is never stored
//   Speed: ~2x faster due to minimizing HBM reads/writes (IO-aware)
//   Output: IDENTICAL to standard attention (mathematically exact, not approximate)
//
//   This is why Flash Attention is the #1 optimization for transformers.
pub fn flash_attention(q: &Tensor, k: &Tensor, v: &Tensor, block_size: usize) -> Tensor {
    let (n, d) = (q.rows, q.cols);
    let mut output = Tensor::zeros(n, d);
    let mut row_max = vec![f32::NEG_INFINITY; n]; // running max for online softmax
    let mut row_sum = vec![0.0_f32; n]; // running sum of exp(scores)

    // Process K,V in blocks
    for kb in (0..n).step_by(block_size) {
        let k_end = (kb + block_size).min(n);

        for qi in 0..n {
            // Compute scores for this Q row against this K block
            for kj in kb..k_end {
                if kj > qi {
                    continue;
                } // causal mask

                let mut score = 0.0;
                for dd in 0..d {
                    score += q.get(qi, dd) * k.get(kj, dd);
                }
                score /= (d as f32).sqrt();

                // Online softmax update
                let prev_max = row_max[qi];
                if score > row_max[qi] {
                    row_max[qi] = score;
                    // Rescale previous accumulated values
                    let correction = (prev_max - score).exp();
                    row_sum[qi] *= correction;
                    for dd in 0..d {
                        let prev = output.get(qi, dd);
                        output.set(qi, dd, prev * correction);
                    }
                }

                let w = (score - row_max[qi]).exp();
                row_sum[qi] += w;
                for dd in 0..d {
                    let prev = output.get(qi, dd);
                    output.set(qi, dd, prev + w * v.get(kj, dd));
                }
            }
        }
    }

    // Normalize
    for i in 0..n {
        for j in 0..d {
            let v = output.get(i, j) / row_sum[i];
            output.set(i, j, v);
        }
    }
    output
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn extract_cols(t: &Tensor, start: usize, end: usize) -> Tensor {
    let cols = end - start;
    let mut result = Tensor::zeros(t.rows, cols);
    for i in 0..t.rows {
        for j in 0..cols {
            result.set(i, j, t.get(i, start + j));
        }
    }
    result
}

fn concat_cols(tensors: &[Tensor]) -> Tensor {
    let rows = tensors[0].rows;
    let total_cols: usize = tensors.iter().map(|t| t.cols).sum();
    let mut result = Tensor::zeros(rows, total_cols);
    let mut col_offset = 0;
    for t in tensors {
        for i in 0..rows {
            for j in 0..t.cols {
                result.set(i, col_offset + j, t.get(i, j));
            }
        }
        col_offset += t.cols;
    }
    result
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Attention Mechanisms ═══\n");

    let seq_len = 4;
    let d_model = 8;

    // Scaled dot-product attention
    let q = Tensor::rand(seq_len, d_model);
    let k = Tensor::rand(seq_len, d_model);
    let v = Tensor::rand(seq_len, d_model);
    let out = scaled_dot_product_attention(&q, &k, &v, None);
    println!(
        "    ScaledDotProduct(Q,K,V): {} → {}",
        q.preview(),
        out.preview()
    );

    // Causal self-attention
    let out = causal_self_attention(&q, &k, &v);
    println!("    CausalSelfAttn(Q,K,V):  {}", out.preview());

    // Multi-head attention
    let mha = MultiHeadAttention::new(d_model, 2);
    let x = Tensor::rand(seq_len, d_model);
    let out = mha.forward(&x, true);
    println!("    MultiHeadAttn(x, 2 heads, causal): {}", out.preview());

    // GQA
    let num_q_heads = 4;
    let num_kv_heads = 2;
    let d_head = 4;
    let q = Tensor::rand(seq_len, num_q_heads * d_head);
    let k = Tensor::rand(seq_len, num_kv_heads * d_head);
    let v = Tensor::rand(seq_len, num_kv_heads * d_head);
    let out = grouped_query_attention(&q, &k, &v, num_q_heads, num_kv_heads, d_head, true);
    println!("    GQA(4 Q heads, 2 KV heads): {}", out.preview());

    // Sliding window
    let mask = sliding_window_mask(4, 2);
    println!("    SlidingWindowMask(seq=4, w=2): row0=[{:.0},{:.0},{:.0},{:.0}] row3=[{:.0},{:.0},{:.0},{:.0}]",
        mask.get(0,0), mask.get(0,1), mask.get(0,2), mask.get(0,3),
        mask.get(3,0), mask.get(3,1), mask.get(3,2), mask.get(3,3));

    // Linear attention
    let q = Tensor::rand(seq_len, 4);
    let k = Tensor::rand(seq_len, 4);
    let v = Tensor::rand(seq_len, 4);
    let out = linear_attention(&q, &k, &v);
    println!("    LinearAttn(O(nd²) not O(n²d)): {}", out.preview());

    // KV Cache
    let mut cache = KVCache::new(4);
    for step in 0..3 {
        let q = vec![0.1 * step as f32; 4];
        let k = vec![0.2 * step as f32; 4];
        let v = vec![1.0 + step as f32; 4];
        let out = cache.attend(&q, &k, &v);
        if step == 2 {
            println!(
                "    KVCache(step=2, cache_len=3): [{:.2}, {:.2}, ...]",
                out[0], out[1]
            );
        }
    }

    // RoPE
    let x = Tensor::rand(seq_len, d_model);
    let rotated = apply_rope(&x, 0);
    println!(
        "    RoPE(x, pos=0): {} → {}",
        x.preview(),
        rotated.preview()
    );

    // Flash Attention
    let q = Tensor::rand(seq_len, 4);
    let k = Tensor::rand(seq_len, 4);
    let v = Tensor::rand(seq_len, 4);
    let out_std = causal_self_attention(&q, &k, &v);
    let out_flash = flash_attention(&q, &k, &v, 2);
    let diff: f32 = out_std
        .data
        .iter()
        .zip(&out_flash.data)
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / out_std.data.len() as f32;
    println!(
        "    FlashAttn vs standard: avg diff = {:.6} (should be ~0)\n",
        diff
    );
}
