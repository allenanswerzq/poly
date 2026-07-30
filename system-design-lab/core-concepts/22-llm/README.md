# Large Language Models — From Word Vectors to GPT and Beyond

A deep-dive into how LLMs work, from the foundational building blocks (embeddings,
attention, transformers) through training, alignment, inference optimization, and
the techniques that make modern AI assistants possible.

---

## 1. The Road to LLMs — A Brief History

```
Timeline:

  2013          2017           2018        2019       2020        2022         2023+
   │             │              │           │          │           │            │
 Word2Vec    Attention      BERT       GPT-2      GPT-3      ChatGPT      GPT-4,
 GloVe      Is All You     (encoder)  (1.5B)     (175B)     InstructGPT   Llama,
             Need           ELMo                  few-shot   RLHF         Mixtral,
             Transformer                          learning                Claude
```

**Key transitions:**
- **Before 2013**: Bag-of-words, TF-IDF — words as sparse vectors, no semantics
- **2013 — Word2Vec**: Words as dense vectors in semantic space
- **2017 — Transformer**: Attention replaces recurrence entirely
- **2018 — BERT/GPT**: Pretrain on huge corpora, then fine-tune for any task
- **2020+ — Scale is all you need**: Bigger models + more data = emergent capabilities
- **2022+ — Alignment**: RLHF makes models helpful, harmless, and honest

---

## 2. Word Embeddings — Making Words into Numbers

### 2.1 The Problem

Computers need numbers. But words aren't numbers. How do you represent "cat" in
a way that captures its meaning?

**One-hot encoding** doesn't work for NLP at scale:
```
"cat"  → [0, 0, 0, 1, 0, 0, 0, ..., 0]   (50,000-dimensional!)
"dog"  → [0, 0, 1, 0, 0, 0, 0, ..., 0]
"fish" → [0, 0, 0, 0, 0, 1, 0, ..., 0]

Problems:
1. Huge vectors (vocabulary size = 50K+)
2. All words are equidistant: dist(cat, dog) = dist(cat, democracy)
3. No semantic meaning captured
```

### 2.2 Word2Vec — Distributional Semantics

**Core idea**: "You shall know a word by the company it keeps" (Firth, 1957).

Words that appear in similar contexts should have similar vectors.

**Two variants:**

```
CBOW (Continuous Bag of Words):      Skip-gram:

  Context → predict center word       Center word → predict context

  "The [cat] sat on the [mat]"        "The cat [sat] on the mat"
       ↓                                    ↓
   [The, sat, on, the]                   [sat]
       ↓                                    ↓
   Predict: "cat"                    Predict: "The", "cat", "on", "the"
```

**Skip-gram training** (the more popular variant):

Given center word $w$ and context word $c$, maximize:

$$P(c \mid w) = \frac{\exp(\mathbf{v}_c \cdot \mathbf{v}_w)}{\sum_{c' \in V} \exp(\mathbf{v}_{c'} \cdot \mathbf{v}_w)}$$

The denominator sums over the ENTIRE vocabulary — too expensive. Solution:
**Negative sampling** — only update a few random "negative" words instead.

**The magical result** — vector arithmetic captures analogies:

```
  king - man + woman ≈ queen
  Paris - France + Germany ≈ Berlin
  walked - walking + swimming ≈ swam

  These arithmetic relationships emerge automatically from co-occurrence patterns!
```

**Typical dimensions**: 100-300 per word vector. Captures synonyms, analogies,
and topical relationships.

### 2.3 GloVe — Global Vectors

Word2Vec only uses local context windows. **GloVe** (Pennington et al., 2014) also
uses the global co-occurrence matrix:

$$J = \sum_{i,j=1}^{V} f(X_{ij})(w_i^T \tilde{w}_j + b_i + \tilde{b}_j - \log X_{ij})^2$$

Where $X_{ij}$ = how often word $i$ appears near word $j$ across the entire corpus.

GloVe combines the best of count-based methods (LSA) and prediction-based methods
(Word2Vec). In practice, results are similar.

### 2.4 Subword Tokenization — Modern Approach

Word-level embeddings can't handle new words ("unfriendliness", "COVID-19").

**Byte Pair Encoding (BPE)** — used by GPT:
```
Start with characters: ["u", "n", "f", "r", "i", "e", "n", "d", "l", "y"]

Merge most frequent pairs iteratively:
  Step 1: "i" + "n" → "in"           (most frequent pair)
  Step 2: "f" + "r" → "fr"
  Step 3: "fr" + "i" → "fri"
  Step 4: "fri" + "e" → "frie"
  Step 5: "frie" + "n" → "frien"
  Step 6: "frien" + "d" → "friend"
  Step 7: "un" + "friend" → "unfriend"
  ...

Final: "unfriend" + "ly" → ["unfriend", "ly"]

Every word can be decomposed into known subwords.
No more "unknown word" problem!
```

**Other tokenizers:**
| Tokenizer | Used by | Key feature |
|-----------|---------|-------------|
| **BPE** | GPT, LLaMA | Frequency-based merging |
| **WordPiece** | BERT | Likelihood-based merging |
| **SentencePiece** | T5, LLaMA | Language-agnostic, works on raw text |
| **Unigram** | Part of SentencePiece | Probabilistic, starts big and prunes |

**Typical vocabulary size**: 32K–100K tokens. GPT-4 uses ~100K.

---

## 3. Attention — The Key Innovation

### 3.1 Why Attention?

RNNs process tokens sequentially: token 1, then token 2, then token 3, ...

Two fundamental problems:
1. **Sequential bottleneck**: Can't parallelize (each step depends on previous)
2. **Long-range forgetting**: Information from token 1 is diluted by the time
   you reach token 1000

Attention solves both: **every token can directly attend to every other token**,
in parallel.

### 3.2 Self-Attention Mechanism

Given a sequence of token embeddings $x_1, x_2, \ldots, x_n$, self-attention
computes a new representation for each token as a **weighted average of all tokens**.

```
Step 1: Create Query, Key, Value vectors for each token

  Token "sat" → embedding x
                  ↓
           ┌──────┼──────┐
           ↓      ↓      ↓
       Q = W_Q·x  K = W_K·x  V = W_V·x

  Q (Query): "What am I looking for?"
  K (Key):   "What do I contain?"
  V (Value): "What information do I provide?"
```

```
Step 2: Compute attention scores

  Score(Q_i, K_j) = Q_i · K_j / √d_k

  ┌─────────────────────────────────────┐
  │       K₁    K₂    K₃    K₄         │
  │  Q₁ [ 0.8   0.1   0.3   0.2 ]      │
  │  Q₂ [ 0.1   0.7   0.5   0.1 ]      │  → dot products
  │  Q₃ [ 0.3   0.5   0.9   0.2 ]      │     (how relevant is each key?)
  │  Q₄ [ 0.2   0.1   0.2   0.8 ]      │
  └─────────────────────────────────────┘

  The /√d_k factor prevents scores from getting too large
  (which would make softmax too peaked → gradients vanish).
```

```
Step 3: Softmax → attention weights (rows sum to 1)

  ┌─────────────────────────────┐
  │  "The"  "cat"  "sat"  "on" │
  │  [0.5    0.1    0.3    0.1] │  ← weights for token 1
  │  [0.1    0.6    0.2    0.1] │  ← weights for token 2
  │  ...                        │
  └─────────────────────────────┘

Step 4: Weighted sum of Values

  Output₁ = 0.5·V₁ + 0.1·V₂ + 0.3·V₃ + 0.1·V₄
```

**The full formula:**

$$\text{Attention}(Q, K, V) = \text{softmax}\left(\frac{QK^T}{\sqrt{d_k}}\right)V$$

This is a single matrix operation. Highly parallelizable on GPUs.

### 3.3 Multi-Head Attention

One attention head captures one type of relationship. Multiple heads capture
multiple types simultaneously:

```
Multi-Head Attention:

  Input X
    │
    ├──→ Head 1: [Q₁, K₁, V₁] → Attention₁  (maybe: syntax)
    ├──→ Head 2: [Q₂, K₂, V₂] → Attention₂  (maybe: coreference)
    ├──→ Head 3: [Q₃, K₃, V₃] → Attention₃  (maybe: semantics)
    └──→ Head h: [Qₕ, Kₕ, Vₕ] → Attentionₕ  (maybe: position)
                                     │
                            Concat all heads
                                     │
                              Linear projection
                                     ↓
                                  Output

  Each head has its own W_Q, W_K, W_V matrices.
  Head dimension = d_model / num_heads (e.g., 768/12 = 64)
```

**Typical**: GPT-3 uses 96 heads with $d_{head}=128$, so $d_{model}=12288$.

### 3.4 Masked (Causal) Attention

For language generation, a token should only attend to **previous** tokens (can't
see the future!):

```
Causal mask:

           Attending TO →
           The  cat  sat  on
  From ↓
  The   [  ✓    ✗    ✗    ✗  ]     Upper triangle
  cat   [  ✓    ✓    ✗    ✗  ]     is masked
  sat   [  ✓    ✓    ✓    ✗  ]     (-∞ before softmax → 0 weight)
  on    [  ✓    ✓    ✓    ✓  ]

  Each token can ONLY attend to itself and earlier tokens.
  This is what makes autoregressive generation possible.
```

### 3.5 Cross-Attention

Used in encoder-decoder models (T5, original Transformer for translation):

```
  Encoder output → provides K, V
  Decoder state  → provides Q

  The decoder "queries" the encoder to find relevant source information.
  This is how a translation model knows which source words to look at
  when generating each target word.
```

---

## 4. The Transformer Architecture

### 4.1 Full Architecture

```
Original Transformer (Vaswani et al., 2017):

┌─────────── ENCODER ───────────┐   ┌─────────── DECODER ───────────┐
│                                │   │                                │
│  Input Embeddings              │   │  Output Embeddings (shifted)   │
│       +                        │   │       +                        │
│  Positional Encoding           │   │  Positional Encoding           │
│       ↓                        │   │       ↓                        │
│  ┌─────────────────────┐       │   │  ┌─────────────────────┐       │
│  │ Multi-Head           │      │   │  │ Masked Multi-Head    │      │
│  │ Self-Attention       │      │   │  │ Self-Attention       │      │
│  │ + Add & LayerNorm    │      │   │  │ + Add & LayerNorm    │      │
│  ├─────────────────────┤       │   │  ├─────────────────────┤       │
│  │ Feed-Forward         │      │   │  │ Multi-Head            │     │
│  │ Network (FFN)        │  ×N  │   │  │ Cross-Attention  ←────┼─────┤
│  │ + Add & LayerNorm    │      │   │  │ + Add & LayerNorm    │      │
│  └─────────────────────┘       │   │  ├─────────────────────┤       │
│                                │   │  │ Feed-Forward         │ ×N   │
│  Output: contextual            │   │  │ Network (FFN)        │      │
│  representations for           │   │  │ + Add & LayerNorm    │      │
│  each input token              │   │  └─────────────────────┘       │
│                                │   │                                │
└────────────────────────────────┘   │  Linear → Softmax → next token │
                                     └────────────────────────────────┘
```

### 4.2 Each Component Explained

**Positional Encoding** — Attention has no notion of order. We must inject
position information:

$$PE_{(pos, 2i)} = \sin\left(\frac{pos}{10000^{2i/d}}\right)$$
$$PE_{(pos, 2i+1)} = \cos\left(\frac{pos}{10000^{2i/d}}\right)$$

Why sinusoidal? Because $PE_{pos+k}$ can be represented as a linear function of
$PE_{pos}$, so the model can learn to attend to relative positions. Modern LLMs
use **Rotary Position Embeddings (RoPE)** instead — encodes relative position
directly into the attention computation, generalizes better to longer sequences.

**Feed-Forward Network (FFN)** — Applied to each position independently:

$$\text{FFN}(x) = W_2 \cdot \text{GELU}(W_1 \cdot x + b_1) + b_2$$

- $W_1$: $d_{model} \to 4 \times d_{model}$ (expand)
- $W_2$: $4 \times d_{model} \to d_{model}$ (compress back)

This is where most of the model's **knowledge is stored** — factual recall happens
in the FFN layers. Attention is about routing; FFN is about processing.

**Residual Connections + Layer Normalization**:

$$\text{output} = \text{LayerNorm}(x + \text{Sublayer}(x))$$

Residuals ensure gradients flow. LayerNorm stabilizes training at each layer.

Modern LLMs use **Pre-Norm** (LayerNorm before sublayer) instead of **Post-Norm**
(after) — it's more stable for very deep models.

### 4.3 Encoder-Only vs. Decoder-Only vs. Encoder-Decoder

```
┌─────────────────────────────────────────────────────────────┐
│ Architecture      │ Attention   │ Models         │ Use case │
├───────────────────┼─────────────┼────────────────┼──────────┤
│ Encoder-only      │ Bidirect.   │ BERT, RoBERTa  │ Classify │
│ (sees all tokens) │ (full attn) │ DeBERTa        │ NER, QA  │
├───────────────────┼─────────────┼────────────────┼──────────┤
│ Decoder-only      │ Causal      │ GPT, LLaMA,    │ Generate │
│ (left-to-right)   │ (masked)    │ Claude, Gemini │ Chat, QA │
├───────────────────┼─────────────┼────────────────┼──────────┤
│ Encoder-Decoder   │ Both        │ T5, BART,      │ Translate│
│ (source→target)   │             │ Whisper        │ Summarize│
└─────────────────────────────────────────────────────────────┘
```

**Decoder-only dominates** because:
1. Simpler architecture (one stack, not two)
2. Scales more predictably
3. Unified framework: any task can be framed as "predict next token"
4. In-context learning emerges naturally from autoregressive pretraining

---

## 5. Training LLMs

### 5.1 Pretraining — Learning Language

**Objective**: Given previous tokens, predict the next one (autoregressive LM):

$$\max_\theta \sum_{t=1}^{T} \log P(x_t \mid x_1, \ldots, x_{t-1}; \theta)$$

```
Training data (internet text):

  "The capital of France is Paris. It is known for the Eiffel Tower."

  Input:  "The capital of France is"
  Target: "capital of France is Paris"

  Each position predicts the next token. One sequence gives you T training examples.
```

**Scale of pretraining:**

| Model | Parameters | Training tokens | Training cost |
|-------|-----------|----------------|---------------|
| GPT-2 | 1.5B | ~40B | ~$50K |
| GPT-3 | 175B | 300B | ~$4.6M |
| LLaMA-2 70B | 70B | 2T | ~$2M |
| GPT-4 | ~1.8T (rumored MoE) | ~13T | ~$100M |
| LLaMA-3 405B | 405B | 15T | ~$30M+ |

**Data** is the most important ingredient:

```
Typical pretraining data mix:

  Web crawl (CommonCrawl)  ████████████████████  67%
  Books                    ████                  13%
  Wikipedia                ██                     6%
  Code (GitHub)            ███                    9%
  Scientific papers        █                      3%
  Social media / forums    █                      2%
```

### 5.2 Supervised Fine-Tuning (SFT)

After pretraining, the model can complete text but doesn't follow instructions.
Fine-tune on (instruction, response) pairs:

```
Pretraining data:  "The Eiffel Tower was built in 1889 and stands..."
                   (Wikipedia-style continuations)

SFT data:          User: "When was the Eiffel Tower built?"
                   Assistant: "The Eiffel Tower was built in 1889..."
                   (instruction-following format)

The model learns to be HELPFUL rather than just completing text.
```

**Typical SFT datasets**: 10K–100K high-quality instruction-response pairs.
Quality >>> quantity here. A small, carefully curated dataset often beats a
large noisy one.

### 5.3 RLHF — Reinforcement Learning from Human Feedback

SFT makes the model helpful but it can still be wordy, hallucinate, or give
harmful outputs. RLHF aligns the model with human preferences.

```
RLHF Pipeline:

Step 1: Collect comparisons

  Prompt: "Explain quantum computing"

  Response A: "Quantum computing uses qubits which can be..."  ← human prefers
  Response B: "As an AI language model, quantum computing..."  ← human dislikes

  Human labels: A > B

Step 2: Train a Reward Model (RM)

  RM(prompt, response) → scalar score

  Trained on comparison data:
  RM(prompt, A) > RM(prompt, B)

  Loss: -log(σ(RM(A) - RM(B)))  (Bradley-Terry model)

Step 3: Optimize policy with PPO

  ┌──────────────────────────────────────────────┐
  │                                              │
  │  Prompt ──→ LLM ──→ Response ──→ RM ──→ Reward
  │                 ↑                         │  │
  │                 └──── PPO Update ←────────┘  │
  │                                              │
  │  Plus KL penalty: don't drift too far        │
  │  from the SFT model                          │
  └──────────────────────────────────────────────┘

  The model learns to generate responses that the reward model scores highly,
  while staying close to its original behavior (KL constraint).
```

**DPO — Direct Preference Optimization** (simpler alternative to RLHF):

Skip the reward model entirely. Directly optimize the policy using:

$$L_{DPO} = -\log\sigma\left(\beta \log\frac{\pi_\theta(y_w|x)}{\pi_{ref}(y_w|x)} - \beta \log\frac{\pi_\theta(y_l|x)}{\pi_{ref}(y_l|x)}\right)$$

Where $y_w$ = preferred response, $y_l$ = dispreferred. This is mathematically
equivalent to RLHF under certain assumptions but avoids training a separate
reward model. Increasingly popular (LLaMA-3, Zephyr).

### 5.4 The Full Training Pipeline

```
Step 1: Pretraining           Step 2: SFT              Step 3: RLHF/DPO
(Months, $M)                  (Hours, $K)              (Days, $K)

Trillions of tokens      →    ~100K instruction    →    Human preferences
Unsupervised next-token       follow pairs              Reward model + PPO
                                                        or Direct DPO

Output: Base model            Output: Chat model        Output: Aligned model
(e.g., LLaMA-3 Base)         (follows instructions)    (helpful, harmless)
```

---

## 6. How Text Generation Works

### 6.1 Autoregressive Generation

```
Prompt: "The weather today is"

Step 1: Process prompt, predict next token
  P("sunny" | "The weather today is") = 0.3
  P("rainy" | "The weather today is") = 0.2
  P("cold"  | "The weather today is") = 0.15
  ...

Step 2: Sample or select a token (say "sunny")

Step 3: Append and repeat
  "The weather today is sunny"
  → predict next token → "and" → ...
  "The weather today is sunny and"
  → predict next token → "warm" → ...

Continue until <EOS> token or max length.
```

### 6.2 Decoding Strategies

| Strategy | How it works | Pros | Cons |
|----------|-------------|------|------|
| **Greedy** | Always pick highest probability token | Deterministic | Repetitive, boring |
| **Beam search** | Track top-k sequences | Better overall probability | Still repetitive |
| **Temperature sampling** | Sample from $P^{1/T}$ distribution | Creative | Can be incoherent |
| **Top-k** | Sample from top k tokens only | Bounded randomness | Fixed k for all distributions |
| **Top-p (nucleus)** | Sample from smallest set with cumulative prob ≥ p | Adaptive | Slightly slower |
| **Min-p** | Only consider tokens with $P > p \times P_{max}$ | Simple, adaptive | Newer, less tested |

```
Temperature effect on probability distribution:

  T=0.1 (sharp):    T=1.0 (normal):    T=2.0 (flat):

  P ▲                P ▲                P ▲
    │█                 │█                 │▓ ▓
    │█                 │█ ▓               │▓ ▓ ▓ ▓
    │█                 │█ ▓ ░ ░           │▓ ▓ ▓ ▓ ░ ░
    └──────            └──────            └──────
     tokens             tokens             tokens

  Low T → almost greedy        High T → almost uniform
```

**In practice**: Temperature 0.7 + top-p 0.9 is a common default. Factual tasks
use lower temperature. Creative tasks use higher.

### 6.3 The KV Cache — Why Inference is Efficient

Without caching, generating $n$ tokens requires $O(n^2)$ compute (recompute
attention for all previous tokens at each step).

```
Without KV cache:                With KV cache:

Step 1: Compute [A B C]         Step 1: Compute [A B C], cache K,V
Step 2: Compute [A B C D]       Step 2: Compute [D] only, use cached K,V
Step 3: Compute [A B C D E]     Step 3: Compute [E] only, use cached K,V

Redundant computation!          Only compute the NEW token's attention!

Cost per step: O(n)             Cost per step: O(1) for self, O(n) for attn
Total for n tokens: O(n²)       Total for n tokens: O(n)
```

**But memory is the bottleneck**: KV cache for a 70B model with 32K context =
~40 GB of GPU memory just for the cache. This is why long-context models are
expensive.

### 6.4 KV Cache — Full Walkthrough (From Prompt to Generation)

**Step 1: Input → Embeddings**

```
Prompt: "The weather today is"

Tokenize: ["The", "weather", "today", "is"] → [464, 8345, 2060, 318]
                                                 (token IDs from vocabulary)

Embedding lookup (from trained weight matrix):
  token 464  → [0.12, -0.34, 0.56, ...] (4096-dim vector)
  token 8345 → [0.78, 0.23, -0.11, ...]
  token 2060 → [-0.45, 0.67, 0.33, ...]
  token 318  → [0.91, -0.12, 0.44, ...]

Now you have a matrix: 4 tokens × 4096 dimensions.
```

**Step 2: Where Q, K, V Come From**

```
Each transformer layer has attention. For EACH token's embedding,
the layer computes THREE vectors using trained weight matrices:

  Q = embedding × W_Q    (Query:  "what am I looking for?")
  K = embedding × W_K    (Key:    "what do I contain?")
  V = embedding × W_V    (Value:  "what information do I provide?")

  W_Q, W_K, W_V are trained weight matrices (fixed after training).

For our 4 tokens:
  ┌────────────────────────────────────────────────────────────────┐
  │  Token          │ Q (query)        │ K (key)        │ V (value)│
  ├─────────────────┼──────────────────┼────────────────┼──────────┤
  │ "The"           │ q₀ = emb₀ × W_Q │ k₀ = emb₀ × W_K│ v₀      │
  │ "weather"       │ q₁ = emb₁ × W_Q │ k₁ = emb₁ × W_K│ v₁      │
  │ "today"         │ q₂ = emb₂ × W_Q │ k₂ = emb₂ × W_K│ v₂      │
  │ "is"            │ q₃ = emb₃ × W_Q │ k₃ = emb₃ × W_K│ v₃      │
  └─────────────────┴──────────────────┴────────────────┴──────────┘

  This is just matrix multiplication. Embeddings × Weights = Q, K, V.
```

**Step 3: Attention Computation**

```
Attention = "for each token, look at all previous tokens and decide
             which ones are relevant"

Due to causal masking, each token can ONLY attend to itself and earlier tokens.

For token "The" (position 0) — can only see itself:

  q₀ · k₀ / √d → score₀₀
  weights = softmax([score₀₀]) = [1.00]
                                   ↑
                                  "The"
  output₀ = 1.00 × v₀
           = just its own value (no context yet, first token)


For token "weather" (position 1) — sees "The" and itself:

  q₁ · k₀ / √d → score₁₀     ("weather" asks: "how relevant is 'The'?")
  q₁ · k₁ / √d → score₁₁     ("weather" asks: "how relevant is 'weather'?")

  weights = softmax([score₁₀, score₁₁]) = [0.30, 0.70]
                                             ↑     ↑
                                           "The" "weather"
  output₁ = 0.30 × v₀ + 0.70 × v₁
           = "weather" now has some context from "The"


For token "today" (position 2) — sees "The", "weather", and itself:

  q₂ · k₀ / √d → score₂₀     ("today" asks: "how relevant is 'The'?")
  q₂ · k₁ / √d → score₂₁     ("today" asks: "how relevant is 'weather'?")
  q₂ · k₂ / √d → score₂₂     ("today" asks: "how relevant is 'today'?")

  weights = softmax([score₂₀, score₂₁, score₂₂]) = [0.05, 0.40, 0.55]
                                                       ↑     ↑      ↑
                                                     "The" "weather" "today"
  output₂ = 0.05 × v₀ + 0.40 × v₁ + 0.55 × v₂
           = "today" draws context mostly from "weather" and itself


For token "is" (position 3) — sees all 4 tokens:

  q₃ · k₀ / √d → score₃₀     ("is" asks: "how relevant is 'The'?")
  q₃ · k₁ / √d → score₃₁     ("is" asks: "how relevant is 'weather'?")
  q₃ · k₂ / √d → score₃₂     ("is" asks: "how relevant is 'today'?")
  q₃ · k₃ / √d → score₃₃     ("is" asks: "how relevant is 'is'?")

  weights = softmax([score₃₀, score₃₁, score₃₂, score₃₃]) = [0.05, 0.60, 0.25, 0.10]
                                                                 ↑     ↑      ↑     ↑
                                                               "The" "weather" "today" "is"
  output₃ = 0.05 × v₀ + 0.60 × v₁ + 0.25 × v₂ + 0.10 × v₃
           = "is" draws heavily from "weather" — because "is" needs to know
             WHAT "is" something → the subject "weather" is most relevant


The full attention weight matrix (with causal mask):

           Attending TO →
           "The"   "weather"  "today"   "is"
  From ↓
  "The"     [ 1.00    ✗          ✗        ✗    ]  ← sees only itself
  "weather" [ 0.30    0.70       ✗        ✗    ]  ← sees "The" + self
  "today"   [ 0.05    0.40       0.55     ✗    ]  ← sees 3 tokens
  "is"      [ 0.05    0.60       0.25     0.10 ]  ← sees all 4 tokens

  ✗ = masked (−∞ before softmax → 0 weight → can't see future tokens)
  Each row sums to 1.0 (softmax).
  Later tokens have MORE context (can attend to more previous tokens).

  The Q asks the question, K provides the match, V provides the answer.
  Q·K = "how much should I attend?" V = "what to take from that token."
```

**Step 3.5: Why ALL Tokens Must Be Computed (Not Just the Last One)**

```
You might think: "We only use the last token's output to predict the next word.
Why compute attention for 'The', 'weather', 'today' at all?"

Answer: because there are MANY layers (80 in LLaMA-70B), and each layer's
K,V vectors depend on the PREVIOUS layer's outputs for ALL tokens.

WITH 1 LAYER (hypothetically — you COULD skip other rows):
  "is" attends to raw embeddings: v₀, v₁, v₂, v₃
  These come from the embedding table. No prior computation needed.
  → You could just compute the last row. Other rows are independent.

WITH 80 LAYERS (reality — you CANNOT skip):

  Layer 1:
    "The"     → attention → output₀¹  (just knows about "The")
    "weather" → attention → output₁¹  (knows about "The" + "weather")
    "today"   → attention → output₂¹  (knows about 3 tokens)
    "is"      → attention → output₃¹  (knows about all 4)

  Layer 2 takes layer 1's outputs as input:
    NEW K,V for layer 2:
      k₀² = W_K² × output₀¹   ← NEED output₀¹ (from "The"'s row in layer 1)
      k₁² = W_K² × output₁¹   ← NEED output₁¹ (from "weather"'s row)
      k₂² = W_K² × output₂¹   ← NEED output₂¹ (from "today"'s row)
      v₀² = W_V² × output₀¹   ← same dependency
      v₁² = W_V² × output₁¹
      v₂² = W_V² × output₂¹

    "is" in layer 2 attends to [k₀², k₁², k₂², k₃²]
    These are DIFFERENT from layer 1's K,V — they're ENRICHED.
    If we hadn't computed "weather"'s row in layer 1,
    we wouldn't have output₁¹, so k₁² and v₁² wouldn't exist.

  Layer 3 takes layer 2's outputs... and so on for 80 layers.

  ┌──────────────────────────────────────────────────────────────┐
  │  The chain of dependencies:                                   │
  │                                                               │
  │  Layer 1: compute ALL 4 tokens → 4 outputs                   │
  │       ↓                                                       │
  │  Layer 2: use those 4 outputs → compute new K,V → 4 outputs  │
  │       ↓                                                       │
  │  Layer 3: use those 4 outputs → compute new K,V → 4 outputs  │
  │       ↓                                                       │
  │  ...80 layers...                                              │
  │       ↓                                                       │
  │  Layer 80: output₃⁸⁰ → linear → softmax → "sunny"           │
  │                                                               │
  │  Every token at every layer contributes to the final output.  │
  │  You can't skip any of them.                                  │
  └──────────────────────────────────────────────────────────────┘

What "is" sees at layer 80 vs layer 1:

  Layer 1:
    v₁("weather") = just the raw concept of "weather"
    → "is" knows: the word "weather" appeared

  Layer 80:
    v₁⁸⁰("weather") = 79 layers of enrichment
    → "weather" now encodes: "this is the SUBJECT of a sentence,
       it's modified by 'today', the sentence is asking about
       weather CONDITIONS, common answers include sunny/rainy/cold,
       the grammar expects an adjective next..."

    → "is" attending to this enriched representation gets MUCH
       better predictions than attending to raw embeddings.

This is the whole POINT of deep networks:
  Shallow = pattern matching (what words appeared?)
  Deep = understanding (what does the sentence MEAN?)
```

**Step 4: Generate Next Token — Where KV Cache Saves Work**

```
After all layers process, the last token's output → predict next token:

  output of "is" → linear layer → softmax over vocabulary (50,000 words)
  → P("sunny") = 0.30, P("rainy") = 0.20, ...
  → sample "sunny"

Now the sequence is: "The weather today is sunny"
We need to predict the NEXT token after "sunny."

WITHOUT KV CACHE (naive, wasteful):
  Recompute Q, K, V for ALL 5 tokens from scratch:
    "The"     → q₀, k₀, v₀     ← already computed last time!
    "weather" → q₁, k₁, v₁     ← already computed!
    "today"   → q₂, k₂, v₂     ← already computed!
    "is"      → q₃, k₃, v₃     ← already computed!
    "sunny"   → q₄, k₄, v₄     ← only this is new

  Attention for "sunny" needs: k₀,k₁,k₂,k₃,k₄ and v₀,v₁,v₂,v₃,v₄
  We recomputed k₀-k₃ and v₀-v₃ for NOTHING. Wasted work.

WITH KV CACHE (smart):
  After processing "The weather today is":
    SAVE in cache: k₀,k₁,k₂,k₃ and v₀,v₁,v₂,v₃

  When "sunny" arrives:
    Only compute: q₄, k₄, v₄ for the NEW token
    Append k₄ to cached keys:   [k₀, k₁, k₂, k₃, k₄]
    Append v₄ to cached values: [v₀, v₁, v₂, v₃, v₄]

    Attention: q₄ · [k₀, k₁, k₂, k₃, k₄] → weights → weighted sum of [v₀...v₄]

    SAME result, but only computed Q/K/V for 1 token instead of 5.
```

**Step 5: Full Generation with KV Cache**

```
Step-by-step generation:

  Token 1-4: "The weather today is"  (PREFILL phase)
    Compute all Q, K, V for 4 tokens in parallel (like a batch)
    Cache: K = [k₀,k₁,k₂,k₃], V = [v₀,v₁,v₂,v₃]
    Predict: "sunny"
    THIS STEP IS COMPUTE-BOUND (lots of parallel matrix ops)

  Token 5: "sunny"  (DECODE phase, one token at a time)
    Compute Q, K, V for "sunny" only (1 token!)
    Append to cache: K = [k₀,k₁,k₂,k₃,k₄], V = [v₀,...,v₄]
    q₄ attends to all cached K,V → predict "and"

  Token 6: "and"
    Compute Q, K, V for "and" only
    Append to cache: K = [k₀,...,k₅], V = [v₀,...,v₅]
    q₅ attends to all cached K,V → predict "warm"

  Token 7: "warm"
    Compute Q, K, V for "warm" only
    Append to cache: K = [k₀,...,k₆], V = [v₀,...,v₆]
    q₆ attends to all cached K,V → predict <EOS>

  ┌────────────────────────────────────────────────────────────────┐
  │                    Without cache    │    With cache             │
  ├────────────────────────────────────┼───────────────────────────┤
  │ Token 5: compute K,V for 5 tokens │ compute K,V for 1 token  │
  │ Token 6: compute K,V for 6 tokens │ compute K,V for 1 token  │
  │ Token 7: compute K,V for 7 tokens │ compute K,V for 1 token  │
  │ ...                                │ ...                       │
  │ Token 100: compute for 100 tokens │ compute K,V for 1 token  │
  │                                    │                           │
  │ Total: 5+6+7+...+100 = ~5000 ops │ Total: 96 × 1 = 96 ops  │
  │                                    │ → ~50x less compute!      │
  └────────────────────────────────────┴───────────────────────────┘
```

**Why Prefill vs Decode Are Different Bottlenecks:**

```
  PREFILL (process the entire prompt):
    Input: 4 tokens × W_Q, W_K, W_V → large matrix multiply
    All 4 tokens processed IN PARALLEL (like a batch)
    → COMPUTE-BOUND (lots of useful FLOPs per byte loaded)
    → Tensor Cores busy, GPU cores busy
    → Fast! Processes 1000s of tokens in milliseconds.

  DECODE (generate one token at a time):
    Input: 1 token × W_Q, W_K, W_V → tiny matrix multiply
    Must still load ALL model weights from HBM for this 1 token
    → MEMORY-BOUND (1 FLOP per byte loaded)
    → GPU mostly waiting for HBM, cores idle
    → Slow! ~30ms per token on 70B model.

  This is why "time to first token" (prefill) is fast,
  but "tokens per second" (decode) is slow.
  Different optimizations for each:
    Prefill: Tensor Cores, larger batch, compute optimization
    Decode:  more HBM bandwidth, quantization, speculative decoding
```

**Why KV Cache Eats So Much Memory:**

```
For each token in the sequence, you store K and V vectors.
Per layer, per attention head.

  LLaMA 70B:
    80 layers × 64 heads × 2 (K+V) × 128 dims × 2 bytes (FP16)
    = 2.6 MB per token

  For a 4096-token context:
    4096 × 2.6 MB = ~10 GB of KV cache PER SEQUENCE

  Batch of 16 sequences:
    16 × 10 GB = 160 GB — more than the model weights (140 GB)!

  This is why:
    • vLLM uses PagedAttention (virtual memory for KV cache,
      allocates in blocks instead of contiguous memory)
    • Long contexts (128K tokens) need KV cache compression
    • Quantizing KV cache (FP16 → INT8) cuts memory in half
    • GQA (Grouped Query Attention) shares K,V across heads
      → fewer K,V to store → smaller cache
```

---

## 7. Transformer Variants & Modern Architectures

### 7.1 BERT — Bidirectional Encoder

```
BERT pretraining:

  Task 1: Masked Language Modeling (MLM)

    Input:  "The [MASK] sat on the [MASK]"
    Output: "The  cat   sat on the  mat"

    Randomly mask 15% of tokens, predict them.
    Unlike GPT, BERT sees both left AND right context.

  Task 2: Next Sentence Prediction (NSP)

    [CLS] Sentence A [SEP] Sentence B [SEP]
    → Binary: Is B the actual next sentence after A?

    (Later work showed NSP doesn't help much.)
```

**BERT is great for**: classification, NER, question answering, sentence
embeddings. Not for generation (bidirectional = can't do autoregressive).

### 7.2 GPT Family — Decoder-Only LMs

```
GPT Evolution:

  GPT-1 (2018):  117M params   |  Showed pretraining + fine-tuning works
  GPT-2 (2019):  1.5B params   |  Zero-shot capabilities, "too dangerous to release"
  GPT-3 (2020):  175B params   |  In-context learning, few-shot prompting
  GPT-3.5:       Unknown       |  Code-trained, chat fine-tuned (ChatGPT)
  GPT-4 (2023):  ~1.8T (MoE)   |  Multimodal, massive capability jump
  GPT-4o (2024): Optimized     |  Faster, cheaper, natively multimodal
```

**In-context learning** — GPT-3's breakthrough: giving examples in the prompt
changes the model's behavior WITHOUT updating weights:

```
Zero-shot:
  "Translate English to French: 'Hello, how are you?'"

One-shot:
  "Translate English to French:
   'Good morning' → 'Bonjour'
   'Hello, how are you?' →"

Few-shot:
  "Translate English to French:
   'Good morning' → 'Bonjour'
   'Thank you' → 'Merci'
   'Hello, how are you?' →"

More examples → better performance (up to a point).
This works because the model learned to identify and follow patterns during pretraining.
```

### 7.3 Mixture of Experts (MoE)

Not all parameters need to be active for every token. MoE uses a **router**
to select a subset of "expert" FFN layers per token:

```
MoE Layer:

  Token embedding
       │
       ↓
  ┌─────────┐
  │  Router  │  → Selects top-k experts (typically k=2 out of 8-64)
  │ (linear) │
  └─────────┘
       │
  ┌────┼────┬────┬────┬────┬────┐
  ↓    ↓    ↓    ↓    ↓    ↓    ↓
 [E₁] [E₂] [E₃] [E₄] [E₅] [E₆] [E₇]  [E₈]
  ↑              ↑
  └── selected ──┘
       │
  Weighted sum of selected experts' outputs
       ↓
    Output
```

**Why MoE matters**:
- Total params can be 8× larger than active params
- Mixtral 8×7B: 47B total params but only 13B active per token
- Gets quality of a larger model at the compute cost of a smaller one
- GPT-4 is rumored to use MoE (~1.8T total, ~280B active)

**Challenge**: Load balancing — if all tokens go to the same expert, you waste
the others. Auxiliary loss forces even distribution.

### 7.4 Grouped Query Attention (GQA)

Standard multi-head attention: each head has its own Q, K, V → large KV cache.

```
Multi-Head Attention (MHA):    Grouped Query Attention (GQA):

  H heads, each has Q, K, V     H query heads share K, V in groups

  Q₁ K₁ V₁                      Q₁ ─┐
  Q₂ K₂ V₂                      Q₂ ─┤─ K₁ V₁
  Q₃ K₃ V₃                      Q₃ ─┐
  Q₄ K₄ V₄                      Q₄ ─┤─ K₂ V₂
  Q₅ K₅ V₅                      Q₅ ─┐
  Q₆ K₆ V₆                      Q₆ ─┤─ K₃ V₃
  Q₇ K₇ V₇                      Q₇ ─┐
  Q₈ K₈ V₈                      Q₈ ─┤─ K₄ V₄

  KV cache: 8 × (K+V)           KV cache: 4 × (K+V)  ← 2× smaller!
```

GQA is used by LLaMA-2/3, Gemini. Multi-Query Attention (MQA) is the extreme:
ALL heads share one K, V (used by PaLM, Falcon). GQA is the sweet spot.

### 7.5 Rotary Position Embeddings (RoPE)

The modern standard for position encoding. Encodes **relative** position by
rotating Q and K vectors:

$$f(q_m, m) = R_m q_m, \quad f(k_n, n) = R_n k_n$$

Where $R_m$ is a rotation matrix dependent on position $m$. The attention score
then depends only on the **relative** position $(m-n)$:

$$q_m^T k_n = (R_m q)^T (R_n k) = q^T R_{m-n} k$$

**Benefits**: Generalizes to longer sequences than seen during training (with
techniques like NTK-aware scaling, YaRN). Used by virtually all modern LLMs
(LLaMA, Mistral, Qwen, etc.).

---

## 8. Scaling Laws — Bigger Is (Predictably) Better

### 8.1 Kaplan et al. (2020) — OpenAI Scaling Laws

Model performance (loss) follows **power laws** in three variables:

$$L(N) \propto N^{-\alpha_N}$$
$$L(D) \propto D^{-\alpha_D}$$
$$L(C) \propto C^{-\alpha_C}$$

Where $N$ = parameters, $D$ = data, $C$ = compute.

```
Log-log plot of loss vs. compute:

  Loss
  (log)
  ▲
  │ ○
  │  ○
  │   ○
  │    ○
  │     ○  ← Remarkably straight line on log-log scale!
  │      ○
  │       ○
  │        ○
  └──────────→ Compute (log)

  Double the compute → predictable loss reduction.
  This is why labs invest billions — returns are PREDICTABLE.
```

### 8.2 Chinchilla Scaling (Hoffmann et al., 2022)

OpenAI's original laws said: scale model size faster than data.
Chinchilla showed: **data and parameters should scale equally**.

$$\text{Optimal: } D \approx 20 \times N$$

| Model | Params | Tokens | Tokens/Param | Status |
|-------|--------|--------|--------------|--------|
| GPT-3 | 175B | 300B | 1.7 | Under-trained |
| Chinchilla | 70B | 1.4T | 20 | Optimally trained |
| LLaMA-2 | 70B | 2T | 29 | Over-trained (intentionally) |

**Over-training** (training on more data than "optimal") gives you a smaller model
that's cheaper to run at inference, at the cost of more training compute.
Since training is a one-time cost but inference is ongoing, this tradeoff makes sense.

### 8.3 Emergent Abilities

At sufficient scale, models suddenly gain capabilities they didn't have before:

```
Performance
  ▲
  │                              ╭────
  │                            ╱
  │                          ╱
  │                        ╱
  │────────────────────── •    ← sudden emergence
  │
  │  flat, flat, flat... then JUMP
  │
  └──────────────────────────────→ Model size (log scale)
```

Examples of emergent abilities:
- **Chain-of-thought reasoning**: Models below ~60B can't do it. Above, they can.
- **Multi-step arithmetic**: Sudden jump at ~100B parameters
- **Code generation**: Emerges around 10B+ with code in training data

(Note: Some researchers argue this is a measurement artifact — with better metrics,
improvements are smooth. The debate continues.)

---

## 9. Making LLMs Useful — Techniques & Patterns

### 9.1 Prompt Engineering

The art of getting the model to do what you want through careful input design.

**Key techniques:**

```
1. System prompts — set the persona and rules
   "You are an expert Python developer. Always provide type hints..."

2. Few-shot examples — show the format you want
   "Input: 'I love this movie' → Sentiment: Positive
    Input: 'Terrible acting' → Sentiment: Negative
    Input: 'Not bad at all' → Sentiment: "

3. Chain of Thought (CoT) — force step-by-step reasoning
   "Let's think step by step."

   Without CoT:  "What is 23 × 17?"  → "391" (often wrong)
   With CoT:     "What is 23 × 17? Let's think step by step."
                 → "23 × 17 = 23 × 10 + 23 × 7 = 230 + 161 = 391" ✓

4. Self-consistency — sample multiple times, take majority vote
   Generate 5 CoT answers → take the most common final answer

5. ReAct — interleave reasoning and action
   Think → Act → Observe → Think → Act → ...
```

### 9.2 Retrieval-Augmented Generation (RAG)

LLMs have a knowledge cutoff and hallucinate. RAG fixes this by retrieving
relevant documents before generating:

```
RAG Pipeline:

  User Query: "What's our company's vacation policy?"
       │
       ↓
  ┌─────────────┐
  │  Embedding   │  → Convert query to vector
  │  Model       │
  └──────┬──────┘
         │
         ↓
  ┌─────────────┐     ┌──────────────────┐
  │  Vector      │ ←── │  Document Chunks  │  (pre-indexed)
  │  Database    │     │  with embeddings  │
  │  (search)    │     └──────────────────┘
  └──────┬──────┘
         │ top-k relevant chunks
         ↓
  ┌─────────────────────────────────┐
  │  LLM Prompt:                     │
  │                                  │
  │  Context: [retrieved documents]  │
  │  Question: [user query]         │
  │  Answer based on the context:    │
  └──────────────┬──────────────────┘
                 │
                 ↓
            LLM Response
```

**Key decisions:**
- **Chunk size**: 256–1024 tokens per chunk (smaller = more precise, larger = more context)
- **Embedding model**: all-MiniLM-L6, BGE, E5, text-embedding-3-large
- **Vector DB**: FAISS (local), Pinecone, Weaviate, Qdrant, pgvector
- **Retrieval**: Semantic search, keyword (BM25), or **hybrid** (combine both)
- **Reranking**: Use a cross-encoder to rerank initial results (more accurate but slower)

### 9.3 Fine-Tuning vs. RAG vs. Prompt Engineering

```
When to use what:

  ┌─────────────────────────────────────────────────────────┐
  │                                                         │
  │  Need to change model behavior/style?                   │
  │  → Fine-tuning                                          │
  │                                                         │
  │  Need model to use specific knowledge/data?             │
  │  → RAG                                                  │
  │                                                         │
  │  Need model to follow a specific format?                │
  │  → Prompt engineering (cheap, try first!)                │
  │                                                         │
  │  Need all of the above?                                 │
  │  → Combine: Fine-tune for style + RAG for knowledge     │
  │                                                         │
  └─────────────────────────────────────────────────────────┘

  Cost:     Prompt eng < RAG < Fine-tuning
  Effort:   Prompt eng < RAG < Fine-tuning
  Control:  Prompt eng < RAG < Fine-tuning
```

### 9.4 Function Calling & Agents

LLMs can't do math, search the web, or access databases natively. But they can
**decide** to call tools:

```
Agent Loop:

  User: "What's the weather in Tokyo and should I bring an umbrella?"
       │
       ↓
  ┌───────────────────────────────────────────┐
  │  LLM thinks:                              │
  │  "I need to check the weather. Let me     │
  │   call the weather API."                  │
  │                                           │
  │  Action: weather_api(city="Tokyo")        │
  └───────────────────┬───────────────────────┘
                      │
                      ↓
  ┌───────────────────────────────────────────┐
  │  Tool Result: {"temp": 18, "rain": 0.8}  │
  └───────────────────┬───────────────────────┘
                      │
                      ↓
  ┌───────────────────────────────────────────┐
  │  LLM thinks:                              │
  │  "Rain probability is 80%. I should       │
  │   recommend an umbrella."                 │
  │                                           │
  │  Response: "It's 18°C in Tokyo with an    │
  │  80% chance of rain. Definitely bring     │
  │  an umbrella!"                            │
  └───────────────────────────────────────────┘
```

**Agent frameworks**: LangChain, LlamaIndex, CrewAI, AutoGen.

---

## 10. Efficient Fine-Tuning — LoRA and Friends

### 10.1 The Problem

Full fine-tuning a 70B model requires:
- 70B × 4 bytes (parameters) = 280 GB
- 70B × 4 bytes (gradients) = 280 GB
- 70B × 8 bytes (Adam optimizer states) = 560 GB
- **Total: ~1.1 TB** of GPU memory

That's 14× A100 80GB GPUs just for weights and optimizer. Impractical for most.

### 10.2 LoRA — Low-Rank Adaptation

**Key insight**: The weight update $\Delta W$ during fine-tuning is **low-rank**.
Instead of updating a $d \times d$ matrix (millions of params), decompose the
update into two small matrices:

$$W' = W + \Delta W = W + BA$$

Where $B \in \mathbb{R}^{d \times r}$, $A \in \mathbb{R}^{r \times d}$, and $r \ll d$.

```
Full fine-tuning:                 LoRA:

  W (d×d)                         W (d×d) — FROZEN
  All params updated               │
  d² trainable params              │ + B(d×r) × A(r×d)
                                   │   only r×d×2 trainable
                                   │
  d=4096, params=16.7M            r=16, params=131K  (128× less!)
```

```
LoRA applied to attention:

  Input x ────→ W_q (frozen) ────→ Q
          │                   ↑
          └──→ B_q × A_q ────┘   (low-rank update)
               (trainable)

  Only the small B and A matrices are trained.
  Original weights are completely frozen.
  At inference: merge W' = W + BA → zero overhead!
```

**Typical LoRA rank**: $r = 8$ to $64$. Alpha (scaling factor) typically $2r$.

**QLoRA** — quantize base model to 4-bit, then train LoRA adapters in 16-bit.
Fine-tune a 70B model on a single 48GB GPU!

### 10.3 Other Parameter-Efficient Methods

| Method | Trainable params | Key idea |
|--------|-----------------|----------|
| **LoRA** | ~0.1% of total | Low-rank weight updates |
| **QLoRA** | ~0.1% + 4-bit base | LoRA + quantization |
| **Prefix tuning** | ~0.1% | Learnable prefix tokens |
| **Adapters** | ~1-5% | Small bottleneck layers inserted between frozen layers |
| **Prompt tuning** | ~0.01% | Learnable continuous prompt embeddings |

---

## 11. Inference Optimization — Making LLMs Fast

### 11.1 Quantization

Reduce precision of weights to save memory and speed up computation:

```
Precision levels:

  FP32:  ████████████████████████████████  32 bits  → Baseline
  FP16:  ████████████████                  16 bits  → 2× speedup
  INT8:  ████████                           8 bits  → 4× speedup
  INT4:  ████                               4 bits  → 8× speedup

  A 70B model in FP16: ~140 GB
  A 70B model in INT4:  ~35 GB  → Fits on a single A100!
```

**Quantization methods:**
- **Post-training quantization (PTQ)**: Quantize after training. Simple but some quality loss.
- **GPTQ**: Weight-only quantization using second-order info. High quality INT4.
- **AWQ**: Activation-aware weight quantization. Better than GPTQ for INT4.
- **GGUF/llama.cpp**: CPU-friendly quantization. Run LLMs on laptops.
- **Quantization-aware training (QAT)**: Train with quantization in the loop. Best quality.

### 11.2 Flash Attention

Standard attention materializes the full $n \times n$ attention matrix:

```
Standard Attention:              Flash Attention:

  Compute: Q×Kᵀ (n×n)            Never materialize
  Store: n×n matrix               the full n×n matrix.
  Apply softmax
  Multiply by V                   Tile computation:
                                  Process in blocks,
  Memory: O(n²)                   accumulate online.
  Speed: Memory-bound
                                  Memory: O(n)
                                  Speed: 2-4× faster
```

Flash Attention uses **kernel fusion** and **tiling**:
1. Load a block of Q, K, V into SRAM (fast memory)
2. Compute attention for that block
3. Accumulate using online softmax
4. Never write the $n \times n$ matrix to slow HBM

**Flash Attention 2/3**: Further optimizations. FlashAttention is now standard
in all major frameworks (PyTorch, vLLM, TensorRT-LLM).

### 11.3 Speculative Decoding

Use a small, fast **draft model** to generate candidate tokens, then verify them
in parallel with the large model:

```
Without speculative decoding:      With speculative decoding:

  Large model generates 1 token    Draft model generates 5 tokens FAST
  at a time (slow)                 Large model verifies all 5 in ONE pass

  Step 1: [token 1] → 100ms       Step 1: Draft [t1 t2 t3 t4 t5] → 20ms
  Step 2: [token 2] → 100ms       Step 2: Verify [✓  ✓  ✓  ✗  -] → 100ms
  Step 3: [token 3] → 100ms                Accept: t1, t2, t3
  Step 4: [token 4] → 100ms
  Step 5: [token 5] → 100ms       Total: 120ms for 3 tokens

  Total: 500ms for 5 tokens        Repeat → ~2-3× throughput improvement
```

If the draft model's tokens are rejected, fall back to the large model's
prediction. Provably generates the **exact same** distribution as the large model.

### 11.4 Continuous Batching (vLLM)

Naive batching wastes GPU cycles because sequences finish at different times:

```
Static batching:                 Continuous batching:

  Seq 1: ████████████████        Seq 1: ████████░░░░░░░░
  Seq 2: ██████████████████████  Seq 2: ████████████████████
  Seq 3: ██████████              Seq 3: ██████████░░░░░░░░░░
                                 Seq 4:           ████████████ ← NEW!
  Wait for longest to finish.
  Short sequences waste GPU.     As sequences finish, slot in new ones.
                                 GPU never idles.
```

**PagedAttention** (vLLM): Manage KV cache like virtual memory. KV blocks can be
non-contiguous → no memory fragmentation → 2-4× higher throughput.

---

## 12. Long Context — Extending the Window

LLMs have a **context window** — the maximum number of tokens they can process.

| Model | Context length |
|-------|---------------|
| GPT-3 | 2K–4K |
| GPT-4 | 8K–128K |
| Claude 3.5 | 200K |
| Gemini 1.5 | 1M–10M |
| LLaMA-3.1 | 128K |

**Why is long context hard?**
- Attention is $O(n^2)$ in compute and memory
- Models trained on short context don't generalize to long context
- "Lost in the middle" — models are worse at finding info in the middle of context

**Solutions:**
1. **RoPE scaling**: Interpolate or extrapolate position embeddings (NTK-aware, YaRN)
2. **Sliding window attention**: Each token only attends to a local window + global tokens
3. **Ring attention**: Distribute context across devices, overlap compute and communication
4. **Context compression**: Summarize, select relevant portions, or use RAG instead

---

## 13. Evaluation — Measuring LLM Quality

### 13.1 Benchmarks

| Benchmark | What it tests |
|-----------|--------------|
| **MMLU** | Multi-task knowledge (57 subjects) |
| **HumanEval** | Code generation (Python functions) |
| **HellaSwag** | Commonsense reasoning |
| **TruthfulQA** | Resistance to common misconceptions |
| **GSM8K** | Grade-school math word problems |
| **MATH** | Competition math |
| **ARC** | Science questions (grade school) |
| **WinoGrande** | Coreference resolution |
| **MT-Bench** | Multi-turn conversation quality |
| **Chatbot Arena** | Head-to-head human preferences (Elo rating) |

### 13.2 LLM-as-Judge

Use a strong LLM (GPT-4, Claude) to evaluate outputs of other models. Cheaper
than human evaluation, surprisingly well-correlated.

```
Judge prompt:
  "You will be given a question and two responses A and B.
   Rate which response is better on a scale of 1-10 for
   helpfulness, accuracy, and clarity."

Mitigations for bias:
  - Swap A/B order and average scores
  - Use multiple judge models
  - Calibrate with human annotations
```

---

## 14. Safety & Alignment

### 14.1 Failure Modes

| Problem | Description | Mitigation |
|---------|------------|------------|
| **Hallucination** | Generates plausible but false info | RAG, citations, confidence calibration |
| **Jailbreaks** | Circumventing safety filters | Red-teaming, RLHF, input filtering |
| **Prompt injection** | Malicious instructions in user input | Input sanitization, system prompt isolation |
| **Sycophancy** | Agrees with user even when wrong | Train on disagreement data |
| **Bias** | Reflects training data biases | Debiasing datasets, RLHF |
| **Privacy** | Memorizes training data | Differential privacy, data filtering |

### 14.2 Alignment Techniques

```
The alignment pipeline:

  Pretraining                 Alignment
  (capability)                (safety)
       │                          │
       ├── Scale                  ├── Constitutional AI (Anthropic)
       ├── Data quality           ├── RLHF / DPO
       └── Architecture           ├── Red-teaming
                                  ├── Safety fine-tuning
                                  └── Output filtering
```

**Constitutional AI**: Instead of human feedback, use a set of principles
("constitution") and have the AI critique and revise its own outputs.

---

## 15. The LLM Stack — Putting It All Together

```
Production LLM Application:

  User Request
       │
       ↓
  ┌─────────────────────────────────────────────────────┐
  │                    Gateway Layer                     │
  │  Rate limiting, auth, input validation,             │
  │  prompt injection detection                         │
  └──────────────────────┬──────────────────────────────┘
                         │
                         ↓
  ┌─────────────────────────────────────────────────────┐
  │                 Orchestration Layer                   │
  │  Prompt template, RAG retrieval, tool selection,    │
  │  context management, memory                        │
  └──────────────────────┬──────────────────────────────┘
                         │
                         ↓
  ┌─────────────────────────────────────────────────────┐
  │                   Inference Layer                    │
  │  Model serving (vLLM, TensorRT-LLM, TGI),          │
  │  KV cache management, batching, quantization        │
  └──────────────────────┬──────────────────────────────┘
                         │
                         ↓
  ┌─────────────────────────────────────────────────────┐
  │                   Model Layer                       │
  │  Base model + LoRA adapters, model weights,         │
  │  tokenizer                                         │
  └──────────────────────┬──────────────────────────────┘
                         │
                         ↓
  ┌─────────────────────────────────────────────────────┐
  │                 Infrastructure Layer                 │
  │  GPU cluster (A100/H100), networking (NVLink),      │
  │  storage, monitoring, cost optimization             │
  └─────────────────────────────────────────────────────┘
```

---

## 16. Key Numbers to Know

| Metric | Value |
|--------|-------|
| Attention compute | $O(n^2 d)$ per layer |
| FFN compute | $O(n \cdot d \cdot 4d)$ per layer |
| KV cache per token per layer | $2 \times d \times \text{precision bytes}$ |
| Tokens/second (A100, 70B, FP16) | ~30-50 tok/s per user |
| Tokens/second (H100, 70B, INT8) | ~100-200 tok/s per user |
| Training FLOPS (1T token, 70B model) | ~$6 \times 10^{23}$ |
| Chinchilla-optimal tokens | ~20× parameter count |
| FP16 memory per param | 2 bytes |
| Training memory per param (Adam) | ~18 bytes (param + grad + optimizer) |

---

## 17. Glossary

| Term | Meaning |
|------|---------|
| **Autoregressive** | Generates one token at a time, conditioned on all previous tokens |
| **BPE** | Byte Pair Encoding — subword tokenization algorithm |
| **Causal mask** | Prevents attention to future tokens (used in decoder-only models) |
| **DPO** | Direct Preference Optimization — simpler alternative to RLHF |
| **Embedding** | Dense vector representation of a discrete token |
| **FFN** | Feed-Forward Network — the MLP block in each transformer layer |
| **GQA** | Grouped Query Attention — shares KV heads across Q heads |
| **Hallucination** | Model generates plausible but factually incorrect text |
| **ICL** | In-Context Learning — learning from examples in the prompt |
| **KV cache** | Cached key/value tensors for efficient autoregressive generation |
| **LoRA** | Low-Rank Adaptation — parameter-efficient fine-tuning |
| **MoE** | Mixture of Experts — sparse activation of expert sub-networks |
| **RLHF** | Reinforcement Learning from Human Feedback |
| **RoPE** | Rotary Position Embeddings — encodes relative position |
| **SFT** | Supervised Fine-Tuning — training on instruction-response pairs |
| **Token** | The basic unit of text processing (~3/4 of a word in English) |
| **Top-p** | Nucleus sampling — sample from smallest set with cumulative P ≥ p |
