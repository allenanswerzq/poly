# Machine Learning — From Scratch to Mastery

Everything you need to understand ML at a deep, first-principles level.
Not a "use sklearn and pray" guide — we derive the math, understand the geometry,
and build the intuition for **why** things work.

---

## 1. What Is Machine Learning, Really?

At its core, ML is **function approximation**. You have some unknown function
$f: X \to Y$ that maps inputs to outputs. You don't know $f$, but you have
examples: $(x_1, y_1), (x_2, y_2), \ldots, (x_n, y_n)$.

Your goal: find a function $\hat{f}$ that approximates $f$ well — not just on
training data, but on **unseen** data.

```
Traditional Programming:          Machine Learning:

  Rules  ──┐                       Data    ──┐
            ├──→ [ Program ] → Output        ├──→ [ Learning ] → Rules
  Data   ──┘                       Labels  ──┘
```

**Three types of learning:**

| Type | What you have | Goal | Example |
|------|--------------|------|---------|
| **Supervised** | (input, label) pairs | Predict label for new input | Spam detection |
| **Unsupervised** | inputs only, no labels | Find structure/patterns | Customer segmentation |
| **Reinforcement** | states, actions, rewards | Maximize cumulative reward | Game playing, robotics |

---

## 2. The Math You Actually Need

### 2.1 Linear Algebra — The Language of ML

**Vectors** are points in space. A feature vector $\mathbf{x} = [x_1, x_2, \ldots, x_d]$
is a point in $d$-dimensional space. Every data point is a vector.

**Dot product** measures similarity:

$$\mathbf{a} \cdot \mathbf{b} = \sum_{i=1}^{d} a_i b_i = \|\mathbf{a}\| \|\mathbf{b}\| \cos\theta$$

When $\cos\theta = 1$, vectors point the same way (maximally similar).
When $\cos\theta = 0$, they're orthogonal (unrelated).
When $\cos\theta = -1$, they point opposite ways.

**This is why cosine similarity works for embeddings** — it's literally the angle
between two vectors, normalized by their lengths.

**Matrices** are linear transformations. When you multiply $\mathbf{y} = W\mathbf{x}$,
the matrix $W$ rotates, scales, or projects $\mathbf{x}$ into a new space.

```
Matrix as transformation:

  Input Space          W          Output Space
  ┌─────────┐    ┌─────────┐    ┌─────────┐
  │  •       │    │ rotate  │    │    •     │
  │    •     │ →  │ scale   │ →  │  •       │
  │      •   │    │ project │    │      •   │
  └─────────┘    └─────────┘    └─────────┘

  A neural network layer is just: y = σ(Wx + b)
  That's a linear transformation (Wx + b) followed by
  a nonlinear squish (σ).
```

**Eigenvalues & Eigenvectors**: For a matrix $A$, if $A\mathbf{v} = \lambda \mathbf{v}$,
then $\mathbf{v}$ is an eigenvector — a direction that the transformation only
scales (by $\lambda$), never rotates. This is the heart of PCA.

**Key operations to know cold:**

| Operation | What it does | Where it shows up |
|-----------|-------------|-------------------|
| Matrix multiply | Linear transform | Every neural net layer |
| Transpose | Flip rows ↔ cols | Gradient computation |
| Inverse | Undo transform | Closed-form solutions (normal equation) |
| Determinant | Volume scaling factor | Checking if system is solvable |
| Eigendecomposition | Find principal directions | PCA, spectral methods |
| SVD | Generalized eigendecomp | Dimensionality reduction, recommenders |

### 2.2 Calculus — How Models Learn

**Gradient**: The direction of steepest ascent. For a function $f(x_1, x_2, \ldots, x_d)$:

$$\nabla f = \left[\frac{\partial f}{\partial x_1}, \frac{\partial f}{\partial x_2}, \ldots, \frac{\partial f}{\partial x_d}\right]$$

To **minimize** a loss, you walk **opposite** to the gradient:

$$\theta_{t+1} = \theta_t - \eta \nabla_\theta L(\theta)$$

That's gradient descent. $\eta$ is the learning rate — too high and you overshoot,
too low and you crawl.

**Chain rule** is what makes backpropagation possible:

$$\frac{\partial L}{\partial w_1} = \frac{\partial L}{\partial a_3} \cdot \frac{\partial a_3}{\partial a_2} \cdot \frac{\partial a_2}{\partial w_1}$$

Each "link" in the chain is a layer in your network. Backprop just applies the
chain rule systematically from output to input.

### 2.3 Probability — Reasoning Under Uncertainty

**Bayes' Theorem** — the foundation of statistical ML:

$$P(H \mid D) = \frac{P(D \mid H) \cdot P(H)}{P(D)}$$

- $P(H \mid D)$: **Posterior** — what you believe after seeing data
- $P(D \mid H)$: **Likelihood** — how probable is the data if H is true
- $P(H)$: **Prior** — what you believed before seeing data
- $P(D)$: **Evidence** — normalizing constant

**Maximum Likelihood Estimation (MLE)**: Find parameters $\theta$ that maximize
$P(\text{data} \mid \theta)$. This is what most training procedures actually do.

**Key distributions:**

| Distribution | Domain | Use case |
|-------------|--------|----------|
| Gaussian (Normal) | Continuous, $(-\infty, +\infty)$ | Measurement noise, priors |
| Bernoulli | Binary {0, 1} | Coin flips, binary classification |
| Categorical | Discrete {1, ..., K} | Multi-class classification |
| Softmax | Simplex $(0,1)^K$, sums to 1 | Converting logits → probabilities |

---

## 3. Supervised Learning — The Workhorse

### 3.1 Linear Regression

**Goal**: Fit a line (or hyperplane) to data.

$$\hat{y} = \mathbf{w}^T \mathbf{x} + b = w_1 x_1 + w_2 x_2 + \ldots + w_d x_d + b$$

**Loss function** — Mean Squared Error (MSE):

$$L = \frac{1}{n}\sum_{i=1}^{n}(y_i - \hat{y}_i)^2$$

Why squared? (1) Differentiable everywhere, (2) penalizes large errors more,
(3) corresponds to Gaussian noise assumption via MLE.

**Two ways to solve:**

1. **Closed-form (Normal Equation)**: $\mathbf{w}^* = (X^T X)^{-1} X^T \mathbf{y}$
   - Exact solution, $O(d^3)$ — fine for small $d$, disaster for large $d$
2. **Gradient Descent**: Iteratively update $\mathbf{w}$
   - Works for any $d$, any loss, any model

```
Gradient Descent Intuition:

  Loss
  ▲
  │\
  │ \        You're here → •
  │  \                    ╱
  │   \                  ╱ gradient points uphill
  │    \                ╱
  │     \       ← step opposite to gradient
  │      \    •
  │       \  ╱
  │        \/  ← minimum!
  └──────────────────────→ w

  Update: w ← w - η · ∂L/∂w
```

### 3.2 Logistic Regression (Classification)

Despite the name, this is for **classification**, not regression.

**Key idea**: Run linear regression, then squeeze the output through a sigmoid
to get a probability:

$$P(y=1 \mid \mathbf{x}) = \sigma(\mathbf{w}^T \mathbf{x} + b) = \frac{1}{1 + e^{-(\mathbf{w}^T \mathbf{x} + b)}}$$

```
Sigmoid Function:

  P(y=1)
  1.0 ┤                          ╭────────
      │                        ╱
      │                      ╱
  0.5 ┤─────────────────── •
      │                  ╱
      │                ╱
  0.0 ┤──────────────╯
      └──────────────┬──────────────── z = w·x + b
                     0

  z >> 0  →  P ≈ 1 (confident positive)
  z << 0  →  P ≈ 0 (confident negative)
  z = 0   →  P = 0.5 (decision boundary)
```

**Loss function** — Binary Cross-Entropy:

$$L = -\frac{1}{n}\sum_{i=1}^{n}\left[y_i \log(\hat{p}_i) + (1-y_i)\log(1-\hat{p}_i)\right]$$

Why not MSE? Because MSE + sigmoid creates a non-convex landscape with bad
local minima. Cross-entropy keeps things convex.

### 3.3 Decision Trees

**Idea**: Recursively split the data on features to create a tree of yes/no
questions.

```
                    [Age > 30?]
                   /            \
                Yes              No
               /                  \
        [Income > 50k?]      [Student?]
         /         \          /       \
       Yes         No       Yes       No
        ↓           ↓        ↓         ↓
     Approve     Deny    Approve     Deny
```

**How to choose splits?** Maximize **information gain** (reduction in entropy):

$$\text{Information Gain} = H(\text{parent}) - \sum_i \frac{|S_i|}{|S|} H(S_i)$$

Where entropy:

$$H(S) = -\sum_{c} p_c \log_2 p_c$$

High entropy = mixed classes (uncertain). Low entropy = pure nodes (certain).
Each split should make children more pure.

**Pros**: Interpretable, handles nonlinear boundaries, no feature scaling needed.
**Cons**: Overfits easily, unstable (small data changes → different tree).

### 3.4 Random Forests — Ensembles Fix Overfitting

**Key insight**: One tree overfits. Many "weak" trees, averaged together, generalize well.

```
Training Data
     │
     ├── Bootstrap Sample 1 ──→ Tree 1 ──→ Prediction 1 ──┐
     ├── Bootstrap Sample 2 ──→ Tree 2 ──→ Prediction 2 ──┤
     ├── Bootstrap Sample 3 ──→ Tree 3 ──→ Prediction 3 ──┼──→ Majority Vote
     ├── ...                                                │     or Average
     └── Bootstrap Sample N ──→ Tree N ──→ Prediction N ──┘
```

**Two tricks:**
1. **Bagging** (Bootstrap Aggregation): Train each tree on a random subset of data (with replacement)
2. **Feature randomness**: At each split, only consider a random subset of features ($\sqrt{d}$ for classification)

This **decorrelates** the trees — they make different mistakes, and averaging
cancels out individual errors. Variance drops dramatically.

### 3.5 Support Vector Machines (SVMs)

**Goal**: Find the hyperplane that maximizes the **margin** — the distance between
the decision boundary and the nearest data points.

```
  Feature 2
  ▲
  │    ○ ○              ● ●
  │      ○ ○          ● ● ●
  │        ○ ○     |●  ●
  │          ○ ○   |  ← margin →  |
  │            ○ ○ |              | ● ●
  │              ○ |   hyperplane | ● ●
  │                |              |
  └────────────────┼──────────────┼──────→ Feature 1
                   ↑              ↑
              support vectors (closest points)
```

**The Kernel Trick**: Data not linearly separable? Map it to a higher-dimensional
space where it IS separable — without actually computing the transformation.

$$K(\mathbf{x}_i, \mathbf{x}_j) = \phi(\mathbf{x}_i) \cdot \phi(\mathbf{x}_j)$$

Common kernels:
- **Linear**: $K = \mathbf{x}_i \cdot \mathbf{x}_j$ (just dot product)
- **RBF (Gaussian)**: $K = \exp(-\gamma \|\mathbf{x}_i - \mathbf{x}_j\|^2)$ (infinite-dimensional!)
- **Polynomial**: $K = (\mathbf{x}_i \cdot \mathbf{x}_j + c)^d$

### 3.6 k-Nearest Neighbors (k-NN)

The simplest algorithm: store all training data. To predict, find the $k$ closest
points and vote.

```
  Query point: ?

           ○ ○
         ○   ○    ● ●
        ○  [?]  ● ●
         ○    ● ●
              ●

  k=3: 2 nearest are ●, 1 is ○  →  predict ●
  k=5: 3 nearest are ○, 2 are ●  →  predict ○

  k matters! Small k → overfitting. Large k → underfitting.
```

**Curse of dimensionality**: In high dimensions, all points become roughly
equidistant. k-NN needs exponentially more data as dimensions grow.

### 3.7 Naive Bayes

Apply Bayes' theorem with a "naive" independence assumption:

$$P(y \mid x_1, \ldots, x_d) \propto P(y) \prod_{i=1}^{d} P(x_i \mid y)$$

**Why "naive"?** It assumes features are independent given the class. This is
almost never true (word frequencies are correlated) — but it works shockingly
well for text classification, spam filtering, etc.

**Why it works despite the wrong assumption**: We only need the correct
**ranking** of classes, not the correct probabilities. The independence assumption
can still produce correct rankings.

---

## 4. Neural Networks — Universal Function Approximators

### 4.1 The Perceptron

A single neuron:

$$y = \sigma\left(\sum_{i=1}^{d} w_i x_i + b\right) = \sigma(\mathbf{w}^T \mathbf{x} + b)$$

```
  x₁ ──w₁──┐
            │
  x₂ ──w₂──┼──→ [Σ + b] ──→ [σ] ──→ y
            │
  x₃ ──w₃──┘

  Linear combination → nonlinear activation → output
```

One neuron = logistic regression. The magic comes from **stacking layers**.

### 4.2 Multi-Layer Perceptron (MLP)

```
  Input        Hidden 1       Hidden 2       Output
  Layer        Layer          Layer          Layer

  x₁ ──────→ [h₁₁] ──────→ [h₂₁] ──────→ [ŷ₁]
         ╲  ╱     ╲  ╱          ╲  ╱
          ╲╱       ╲╱            ╲╱
          ╱╲       ╱╲            ╱╲
         ╱  ╲     ╱  ╲          ╱  ╲
  x₂ ──────→ [h₁₂] ──────→ [h₂₂] ──────→ [ŷ₂]
         ╲  ╱     ╲  ╱
          ╲╱       ╲╱
          ╱╲       ╱╲
  x₃ ──────→ [h₁₃] ──────→ [h₂₃]

  Each arrow has a learnable weight.
  Each node applies: output = σ(Σ inputs × weights + bias)
```

**Universal Approximation Theorem**: A single hidden layer with enough neurons
can approximate ANY continuous function to arbitrary precision. But "enough neurons"
might be astronomically large — that's why we use **deep** networks (many layers).

### 4.3 Activation Functions — Why Nonlinearity Matters

Without activation functions, stacking layers is pointless:
$W_2(W_1\mathbf{x}) = (W_2 W_1)\mathbf{x} = W'\mathbf{x}$ — still just a
linear transformation.

Nonlinearity lets the network learn **curved** decision boundaries.

```
ReLU: f(x) = max(0, x)          Sigmoid: f(x) = 1/(1+e^-x)

  y                                y
  ▲      ╱                        1├─────────────╭────────
  │     ╱                          │           ╱
  │    ╱                         .5├─────── •
  │   ╱                            │       ╱
  ├──╱                           0 ├──────╯
  └──────────→ x                   └──────────────→ x

Tanh: f(x) = (e^x - e^-x)/(e^x + e^-x)

  y
  1├                    ╭────────
   │                  ╱
  0├──────────── •
   │           ╱
 -1├──────────╯
   └──────────────→ x
```

| Activation | Range | Pros | Cons | Use when |
|-----------|-------|------|------|----------|
| **ReLU** | $[0, \infty)$ | Fast, no vanishing gradient | Dead neurons (output=0 forever) | Default for hidden layers |
| **Leaky ReLU** | $(-\infty, \infty)$ | No dead neurons | Slightly slower | When you have dying ReLU problems |
| **Sigmoid** | $(0, 1)$ | Probabilistic output | Vanishing gradient, not zero-centered | Output layer (binary classification) |
| **Tanh** | $(-1, 1)$ | Zero-centered | Vanishing gradient | RNNs, when you need negative outputs |
| **GELU** | $\approx(-0.17, \infty)$ | Smooth ReLU variant | Slightly slower | Transformers (default in BERT/GPT) |
| **SiLU/Swish** | $\approx(-0.28, \infty)$ | Self-gated smooth ReLU | Slightly slower | Modern architectures (LLaMA, etc.) |

### 4.4 Backpropagation — How Networks Learn

The algorithm that made deep learning possible. It's just the **chain rule**
applied systematically.

```
Forward pass (compute predictions):
  x → [Layer 1] → a₁ → [Layer 2] → a₂ → [Layer 3] → ŷ → [Loss] → L

Backward pass (compute gradients):
  ∂L/∂w₃ ← ∂L/∂ŷ ← ∂L/∂a₂ ← ∂L/∂a₁ ← ...

  Each layer receives the gradient from the next layer,
  multiplies by its local gradient, and passes it back.
```

**Step by step for a 2-layer network:**

1. **Forward**: $\mathbf{z}_1 = W_1\mathbf{x} + \mathbf{b}_1$, $\mathbf{a}_1 = \sigma(\mathbf{z}_1)$,
   $\mathbf{z}_2 = W_2\mathbf{a}_1 + \mathbf{b}_2$, $\hat{y} = \sigma(\mathbf{z}_2)$, $L = \text{loss}(y, \hat{y})$

2. **Backward**:
   - $\frac{\partial L}{\partial W_2} = \frac{\partial L}{\partial \hat{y}} \cdot \frac{\partial \hat{y}}{\partial \mathbf{z}_2} \cdot \mathbf{a}_1^T$
   - $\frac{\partial L}{\partial W_1} = \frac{\partial L}{\partial \hat{y}} \cdot \frac{\partial \hat{y}}{\partial \mathbf{z}_2} \cdot W_2^T \cdot \frac{\partial \mathbf{a}_1}{\partial \mathbf{z}_1} \cdot \mathbf{x}^T$

3. **Update**: $W_i \leftarrow W_i - \eta \frac{\partial L}{\partial W_i}$

### 4.5 The Vanishing/Exploding Gradient Problem

In deep networks, gradients are **products** of many terms (chain rule). If each
term is $< 1$, the gradient shrinks exponentially → **vanishing gradient** (early
layers stop learning). If each term is $> 1$ → **exploding gradient**.

```
Layer 20      Layer 10      Layer 1
  ∂L           ∂L             ∂L
  ── = 0.7²⁰ × ── ≈ 0.0008 × ──    ← vanishing!
  ∂w           ∂w             ∂w

  Gradient shrinks exponentially as you go deeper.
```

**Solutions:**
- **ReLU activation**: Gradient is 1 for positive inputs (no multiplication decay)
- **Residual connections**: $y = f(x) + x$ — gradient can flow through the skip connection
- **Batch normalization**: Keeps activations in a well-behaved range
- **Careful initialization**: Xavier/He initialization scales weights by $1/\sqrt{n}$
- **Gradient clipping**: Cap gradient magnitude (for exploding gradients)

---

## 5. Convolutional Neural Networks (CNNs)

### 5.1 Why CNNs for Images?

A 224×224 RGB image has **150,528** pixels. A fully-connected layer to 1000
hidden neurons would need **150 million** weights — just for the first layer.

CNNs exploit two properties of images:
1. **Spatial locality**: nearby pixels are related
2. **Translation invariance**: a cat is a cat regardless of position

```
Convolution operation:

  Input Image              Filter (3×3)         Output Feature Map
  ┌─────────────┐          ┌───────┐
  │ 1  0  1  0  │          │ 1 0 1 │
  │ 0 [1  1  0] │    *     │ 0 1 0 │   =        ┌───────┐
  │ 1 [0  1  1] │          │ 1 0 1 │             │ 4 3 . │
  │ 0 [1  0  1] │          └───────┘             │ . . . │
  │ 1  0  0  1  │                                └───────┘
  └─────────────┘
            ↑
     Slide the filter across the image,
     compute dot product at each position.
```

### 5.2 CNN Architecture

```
  Input     Conv+ReLU     Pool     Conv+ReLU     Pool      FC        Output
  Image                                                    Layers

 ┌──────┐  ┌──────────┐  ┌────┐  ┌──────────┐  ┌────┐  ┌────────┐  ┌───┐
 │224×  │→ │ 32 feat  │→ │Down│→ │ 64 feat  │→ │Down│→ │ 4096   │→ │10 │
 │224×3 │  │ maps     │  │sample│ │ maps     │  │sample│ │neurons │  │   │
 └──────┘  └──────────┘  └────┘  └──────────┘  └────┘  └────────┘  └───┘

 Early layers detect edges, textures (low-level features)
 Deeper layers detect parts, objects (high-level features)
```

**Key components:**
- **Convolution layer**: Slides filters across input, learns feature detectors
- **Pooling layer**: Downsamples (max pool: take max in window), reduces size
- **Stride**: How far the filter moves each step (stride=2 halves dimensions)
- **Padding**: Add zeros around edges to preserve spatial dimensions
- **Feature map**: Output of a filter — one per filter (32 filters → 32 feature maps)

### 5.3 Landmark CNN Architectures

| Architecture | Year | Key Innovation | Depth |
|-------------|------|---------------|-------|
| **LeNet** | 1998 | First practical CNN (digits) | 5 layers |
| **AlexNet** | 2012 | ReLU, dropout, GPU training | 8 layers |
| **VGG** | 2014 | Simple 3×3 filters only | 16/19 layers |
| **GoogLeNet** | 2014 | Inception modules (multi-scale) | 22 layers |
| **ResNet** | 2015 | Skip connections → 100+ layers | 50/101/152 |
| **EfficientNet** | 2019 | Compound scaling (depth×width×resolution) | Varies |

**ResNet's skip connection** — the most important architectural innovation:

```
           ┌─────────────────────┐
           │                     │
  x ── → [Conv] → [BN] → [ReLU] → [Conv] → [BN] → (+) → [ReLU] → output
           │                     ↑         ↑
           │                     │         │
           └─────────────────────┘   identity shortcut
                                          x

  output = F(x) + x

  If F(x) is useless, the network can just learn F(x) = 0
  and pass x through. This makes deeper networks NEVER worse
  than shallower ones.
```

---

## 6. Recurrent Neural Networks (RNNs)

### 6.1 Sequences Need Memory

Feedforward networks process each input independently. For sequences (text,
time series, audio), you need to remember previous inputs.

```
RNN unrolled through time:

  x₁         x₂         x₃         x₄
   ↓          ↓          ↓          ↓
┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐
│ RNN  │→ │ RNN  │→ │ RNN  │→ │ RNN  │
│ Cell │  │ Cell │  │ Cell │  │ Cell │
└──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘
   ↓    h₁   ↓   h₂    ↓   h₃    ↓   h₄
   y₁        y₂        y₃        y₄

  All cells share the SAME weights.
  h_t = hidden state = "memory" at time t
  h_t = tanh(W_hh · h_{t-1} + W_xh · x_t + b)
```

**Problem**: Vanilla RNNs can't learn long-range dependencies. After ~10-20
timesteps, the gradient vanishes → the network "forgets" early inputs.

### 6.2 LSTM — Long Short-Term Memory

LSTMs add **gates** — learnable switches that control information flow:

```
LSTM Cell:

                    ┌──────────────────────────────────────┐
                    │            Cell State C_t             │
  C_{t-1} ────────→│──── × ────────── + ──────────────────→│──── C_t
                    │     ↑            ↑                    │
                    │  Forget       Input Gate              │
                    │  Gate f_t     i_t × C̃_t              │
                    │     ↑            ↑                    │
                    │  ┌──┴──┐     ┌──┴──┐                 │
  h_{t-1} ───┐     │  │ σ   │     │σ  tanh│               │
              ├───→ │  └─────┘     └──────┘                │
  x_t ────────┘     │                                      │
                    │           Output Gate                 │
                    │           o_t = σ(...)                │
                    │              ↓                        │
                    │   h_t = o_t × tanh(C_t)              │
                    └──────────────────────────────────────┘

  Forget gate: "What to throw away from memory"     → f_t = σ(W_f · [h_{t-1}, x_t])
  Input gate:  "What new info to store"              → i_t = σ(W_i · [h_{t-1}, x_t])
  Output gate: "What to output from memory"          → o_t = σ(W_o · [h_{t-1}, x_t])
```

**Why it works**: The cell state $C_t$ is like a conveyor belt — gradients can
flow through it with minimal decay (the forget gate just multiplies by ~1).

### 6.3 GRU — Gated Recurrent Unit

A simplified LSTM with 2 gates instead of 3:

- **Reset gate**: How much past to forget
- **Update gate**: How much to update vs keep

Fewer parameters, comparable performance. Often preferred when data is limited.

> **Note**: RNNs (including LSTM/GRU) have been largely superseded by **Transformers**
> for most sequence tasks. Transformers process all positions in parallel (no sequential
> bottleneck) and handle long-range dependencies better. See the LLM section.

---

## 7. Unsupervised Learning

### 7.1 K-Means Clustering

**Goal**: Partition $n$ data points into $k$ clusters.

```
Algorithm:
  1. Initialize k centroids randomly
  2. Assign each point to nearest centroid
  3. Recompute centroids as cluster means
  4. Repeat until convergence

  Iteration 1:       Iteration 2:       Converged:

   ○  ●              ○   ●              ○   ●
  ○ ★  ●            ○  ★  ●           ○  ★   ●
   ○   ● ●           ○    ● ●          ○     ● ●
      ★                  ★                  ★
   △  △ △            △  △ △            △   △ △
    △ △               △  △              △  △

  (★ = centroid, shapes = cluster assignments)
```

**Limitations**: Must specify $k$ upfront. Assumes spherical clusters. Sensitive
to initialization (use k-means++ to fix). Can't find non-convex clusters.

### 7.2 Principal Component Analysis (PCA)

**Goal**: Find the directions of maximum variance in the data. Project onto the
top $k$ directions to reduce dimensionality.

```
Original 2D data:           After PCA:

  y                          PC2
  ▲     • •                  ▲
  │   •  •  •                │  •
  │  • •  •                  │ •• •
  │ •  • •  •                ├─•──•──•──•──→ PC1
  │   •  •                   │  ••
  │  •                       │ •
  └──────────→ x

  PC1 captures the most variance (the "main direction" of the data).
  PC2 captures the remaining variance orthogonal to PC1.
  If PC2 has very little variance, you can drop it → 1D representation.
```

**Algorithm**:
1. Center the data (subtract mean)
2. Compute covariance matrix $C = \frac{1}{n}X^TX$
3. Find eigenvectors/eigenvalues of $C$
4. Sort by eigenvalue (largest = most variance)
5. Project onto top $k$ eigenvectors

**Explained variance ratio** tells you how much information each component captures.
If 2 components capture 95% of variance, those 2 dimensions are "enough."

### 7.3 t-SNE and UMAP

PCA is linear — can't capture curved structures. **t-SNE** and **UMAP** are
nonlinear methods for **visualization** (reduce to 2D/3D).

- **t-SNE**: Preserves local structure (nearby points stay nearby). Slow ($O(n^2)$).
  Great for visualizing clusters but distances between clusters are meaningless.
- **UMAP**: Faster, preserves more global structure. Scales better. Preferred
  for large datasets.

---

## 8. Model Evaluation — How to Know If It Works

### 8.1 The Bias-Variance Tradeoff

```
Total Error = Bias² + Variance + Irreducible Noise

  Error
  ▲
  │╲                          ╱
  │ ╲  Bias²                ╱  Variance
  │  ╲                    ╱
  │   ╲                 ╱
  │    ╲              ╱
  │     ╲    ╱╲     ╱
  │      ╲ ╱   ╲  ╱
  │       ╳     ╲╱
  │      ╱╲     ╱╲    ← Total Error
  │    ╱    ╲ ╱    ╲
  │  ╱       ╳      ╲
  └──────────┼────────────────→ Model Complexity
          Sweet spot

  Simple model: High bias (underfits), low variance
  Complex model: Low bias, high variance (overfits)
```

- **Bias**: Error from wrong assumptions. A linear model has high bias for
  nonlinear data.
- **Variance**: Error from sensitivity to training data. A deep tree memorizes
  noise.

### 8.2 Cross-Validation

Don't evaluate on training data. **Ever.**

```
k-Fold Cross-Validation (k=5):

  Fold 1: [Test | Train | Train | Train | Train]  → score₁
  Fold 2: [Train | Test | Train | Train | Train]  → score₂
  Fold 3: [Train | Train | Test | Train | Train]  → score₃
  Fold 4: [Train | Train | Train | Test | Train]  → score₄
  Fold 5: [Train | Train | Train | Train | Test]  → score₅

  Final score = mean(score₁, ..., score₅)
```

### 8.3 Classification Metrics

For a binary classifier:

```
                        Predicted
                    Positive  Negative
              ┌──────────┬──────────┐
  Actual  Pos │   TP     │   FN     │
              ├──────────┼──────────┤
  Actual  Neg │   FP     │   TN     │
              └──────────┴──────────┘
```

| Metric | Formula | When to use |
|--------|---------|-------------|
| **Accuracy** | $\frac{TP+TN}{TP+TN+FP+FN}$ | Balanced classes only |
| **Precision** | $\frac{TP}{TP+FP}$ | When FP is costly (spam filter) |
| **Recall** | $\frac{TP}{TP+FN}$ | When FN is costly (cancer detection) |
| **F1 Score** | $\frac{2 \cdot P \cdot R}{P + R}$ | Balance precision & recall |
| **AUC-ROC** | Area under ROC curve | Overall discriminating ability |

**Never use accuracy for imbalanced datasets.** If 99% of emails are not spam,
a model that always says "not spam" has 99% accuracy but is useless.

### 8.4 Regression Metrics

| Metric | Formula | Notes |
|--------|---------|-------|
| **MSE** | $\frac{1}{n}\sum(y-\hat{y})^2$ | Penalizes large errors heavily |
| **RMSE** | $\sqrt{MSE}$ | Same units as target |
| **MAE** | $\frac{1}{n}\sum\|y-\hat{y}\|$ | Robust to outliers |
| **R²** | $1 - \frac{SS_{res}}{SS_{tot}}$ | % variance explained (1 = perfect) |

---

## 9. Regularization — Preventing Overfitting

### 9.1 L1 and L2 Regularization

Add a penalty to the loss function to discourage large weights:

- **L2 (Ridge)**: $L + \lambda \sum w_i^2$ — shrinks all weights toward zero
- **L1 (Lasso)**: $L + \lambda \sum |w_i|$ — drives some weights to **exactly** zero (feature selection!)
- **Elastic Net**: $L + \lambda_1 \sum |w_i| + \lambda_2 \sum w_i^2$ — both

```
L1 vs L2 geometry:

  L1 constraint (diamond):        L2 constraint (circle):

       w₂                              w₂
       ▲                                ▲
       │  ╱╲                            │   ╭──╮
       │ ╱  ╲                           │  │    │
  ─────╳─────╳────→ w₁           ──────╳────────→ w₁
       │ ╲  ╱                           │  │    │
       │  ╲╱                            │   ╰──╯

  L1 has corners on axes → solution      L2 is smooth → solution
  tends to land ON an axis (w=0)         rarely lands exactly on axis

  This is why L1 produces sparse weights (feature selection).
```

### 9.2 Dropout

During training, randomly set neurons to zero with probability $p$ (typically 0.1–0.5).

```
Without Dropout:              With Dropout (p=0.3):

  ○ ──── ○ ──── ○              ○ ──── ○ ──── ○
  ○ ──── ○ ──── ○              ○ ──── ╳       ○
  ○ ──── ○ ──── ○              ╳       ○ ──── ○
  ○ ──── ○ ──── ○              ○ ──── ○ ──── ╳

  (╳ = dropped neuron)
```

**Why it works**: Forces the network to not rely on any single neuron. Effectively
trains an **ensemble** of subnetworks. At test time, use all neurons but scale
weights by $(1-p)$.

### 9.3 Batch Normalization

Normalize activations within each mini-batch:

$$\hat{x}_i = \frac{x_i - \mu_B}{\sqrt{\sigma_B^2 + \epsilon}}$$

Then scale and shift with learnable parameters: $y_i = \gamma \hat{x}_i + \beta$

**Effects**: Stabilizes training, allows higher learning rates, provides mild
regularization. Almost universal in modern architectures.

### 9.4 Early Stopping

Monitor validation loss during training. Stop when it starts increasing:

```
  Loss
  ▲
  │╲
  │ ╲        Training loss
  │  ╲           (keeps ↓)
  │   ╲  ╲
  │    ╲   ╲      ╱ Validation loss
  │     ╲   ╲   ╱   (starts ↑)
  │      ╲   ╲╱ ← Stop here!
  │       ╲
  └──────────────────→ Epochs
```

---

## 10. Optimization — Making Training Work

### 10.1 Gradient Descent Variants

| Variant | Update rule | Batch size |
|---------|-------------|------------|
| **Batch GD** | Use ALL data per step | Full dataset |
| **Stochastic GD** | Use ONE sample per step | 1 |
| **Mini-batch GD** | Use a batch (32-512) per step | Mini-batch |

Mini-batch is the standard — gives you noise (helps escape local minima) with
reasonable compute.

### 10.2 Momentum

**Problem**: SGD oscillates in ravines (narrow valleys in the loss landscape).

**Solution**: Accumulate velocity from past gradients:

$$v_t = \beta v_{t-1} + \eta \nabla L$$
$$\theta = \theta - v_t$$

```
Without momentum:               With momentum:

  ╲  ╱╲  ╱╲  ╱╲  ╱              ╲
   ╲╱  ╲╱  ╲╱  ╲╱                ╲
                                   ╲
    Oscillates back and forth        → Smooth, fast path to minimum
```

### 10.3 Adam — The Default Optimizer

Combines momentum AND adaptive learning rates per parameter:

- **First moment** ($m$): Exponential moving average of gradients (like momentum)
- **Second moment** ($v$): Exponential moving average of squared gradients
  (adapts learning rate per parameter)

$$m_t = \beta_1 m_{t-1} + (1-\beta_1) g_t$$
$$v_t = \beta_2 v_{t-1} + (1-\beta_2) g_t^2$$
$$\theta = \theta - \eta \frac{\hat{m}_t}{\sqrt{\hat{v}_t} + \epsilon}$$

Parameters that get large gradients → learning rate decreases automatically.
Parameters that get small gradients → learning rate increases. **This is why
Adam works so well out of the box.**

Default hyperparameters ($\beta_1=0.9$, $\beta_2=0.999$, $\epsilon=10^{-8}$)
almost always work. Start with Adam with learning rate $3 \times 10^{-4}$.

### 10.4 Learning Rate Schedules

| Schedule | Behavior | When to use |
|----------|----------|-------------|
| **Constant** | Fixed LR | Baseline |
| **Step decay** | Reduce by factor every N epochs | Classic CV training |
| **Cosine annealing** | Smooth decrease following cosine curve | Modern default |
| **Warmup + decay** | Start low, increase, then decrease | Transformers, large models |
| **One-cycle** | Increase then decrease within one training run | Fast convergence |

```
Cosine Annealing with Warmup:

  LR
  ▲
  │      ╭──╮
  │     ╱    ╲
  │    ╱      ╲
  │   ╱        ╲
  │  ╱          ╲
  │ ╱            ╲
  │╱              ╲
  └─┬──────────────╲──→ Steps
    ↑               ↑
  Warmup          Cosine decay
```

---

## 11. Feature Engineering & Data Preprocessing

### 11.1 Feature Scaling

Many algorithms are sensitive to feature scales (SVM, k-NN, neural nets, PCA).
Tree-based methods are NOT.

| Method | Formula | Range | When |
|--------|---------|-------|------|
| **Min-Max** | $\frac{x - \min}{\max - \min}$ | [0, 1] | Bounded data, neural nets |
| **Standardization** | $\frac{x - \mu}{\sigma}$ | unbounded, mean=0, std=1 | Gaussian-ish data, SVMs |
| **Robust** | $\frac{x - \text{median}}{IQR}$ | unbounded | Data with outliers |

### 11.2 Handling Missing Data

| Strategy | When to use |
|----------|-------------|
| **Drop rows** | < 5% missing, data is plentiful |
| **Mean/median imputation** | Simple, MCAR assumption |
| **Mode imputation** | Categorical features |
| **KNN imputation** | When neighbors carry signal |
| **Indicator variable** | Missingness itself is informative |

### 11.3 Encoding Categorical Variables

- **One-hot encoding**: Create binary columns per category. Good for < 20 categories.
- **Label encoding**: Assign integers. Only for ordinal data (low/med/high).
- **Target encoding**: Replace category with mean of target. Powerful but prone to leakage — must use cross-validation.
- **Embeddings**: Learn dense vector per category. Best for high-cardinality (user IDs, zip codes).

---

## 12. Ensemble Methods — Beyond Random Forests

### 12.1 Boosting

Build trees sequentially, each one fixing the mistakes of the previous:

```
  Data              Weighted Data        Weighted Data
   │                     │                    │
   ↓                     ↓                    ↓
  Tree 1 → errors → weights ↑ → Tree 2 → errors → weights ↑ → Tree 3
   │                              │                              │
   ↓                              ↓                              ↓
  pred₁    +    α₂ × pred₂    +    α₃ × pred₃   =   Final prediction
```

### 12.2 Gradient Boosting (XGBoost, LightGBM)

Instead of reweighting samples, **fit the new tree on the residuals** (gradient
of the loss) of the previous model.

**XGBoost/LightGBM are still the go-to for tabular data.** They beat neural
networks on most tabular tasks because:
1. Trees naturally handle mixed feature types
2. Built-in feature interactions
3. Robust to outliers and missing values
4. Much faster to train than NNs

| Library | Key innovation |
|---------|---------------|
| **XGBoost** | Regularized objective, column subsampling |
| **LightGBM** | Leaf-wise growth (faster), histogram binning |
| **CatBoost** | Native categorical handling, ordered boosting |

### 12.3 Stacking

Train multiple diverse models, then train a **meta-model** on their predictions:

```
Training data → Model 1 (RF) ──→ pred₁ ─┐
             → Model 2 (XGB) ──→ pred₂ ─┼──→ Meta-model → Final prediction
             → Model 3 (NN) ──→ pred₃ ──┘
```

---

## 13. Practical ML Workflow

```
Step 1: Understand the Problem
  │  What are you predicting? What metric matters?
  │  Is it classification, regression, ranking?
  ↓
Step 2: Explore the Data (EDA)
  │  Distributions, correlations, missing values,
  │  class imbalance, outliers
  ↓
Step 3: Baseline Model
  │  Simplest model that could work.
  │  Linear regression / logistic regression / dummy classifier.
  │  This is your floor — everything must beat this.
  ↓
Step 4: Feature Engineering
  │  Create, transform, select features.
  │  Domain knowledge is king here.
  ↓
Step 5: Model Selection
  │  Try 2-3 model families. Cross-validate.
  │  Tabular data? XGBoost. Images? CNN. Text? Transformer.
  ↓
Step 6: Hyperparameter Tuning
  │  Grid search, random search, or Bayesian optimization.
  │  Tune on validation set, NEVER on test set.
  ↓
Step 7: Evaluate on Test Set
  │  Only once! This is your unbiased estimate.
  ↓
Step 8: Deploy & Monitor
     Model drift, data drift, shadow testing, A/B tests.
```

---

## 14. Quick Reference — When to Use What

| Problem | Algorithm | Why |
|---------|-----------|-----|
| Tabular classification | XGBoost / LightGBM | Best out-of-box for tabular |
| Tabular regression | XGBoost / LightGBM | Same |
| Image classification | CNN (ResNet, EfficientNet) | Spatial invariance |
| Object detection | YOLO, Faster R-CNN | Localization + classification |
| Text classification | Fine-tuned Transformer | BERT, RoBERTa |
| Time series | Temporal Fusion Transformer, LSTM | Sequential patterns |
| Anomaly detection | Isolation Forest, Autoencoder | Works with unlabeled data |
| Clustering | K-Means, DBSCAN, HDBSCAN | Group discovery |
| Dimensionality reduction | PCA (linear), UMAP (nonlinear) | Visualization, preprocessing |
| Recommendation | Matrix factorization, two-tower NN | User-item interactions |
| Ranking | LambdaMART, learning-to-rank | Search, recommendations |

---

## 15. Key Concepts to Internalize

1. **No Free Lunch**: No single algorithm works best for all problems. Domain
   knowledge + experimentation > blindly applying models.

2. **Data > Model**: A simple model with great data beats a complex model with
   bad data. Spend 80% of time on data, 20% on modeling.

3. **Occam's Razor**: Start with the simplest model. Only add complexity if
   validation performance demands it.

4. **Feature Engineering is Art**: The best ML engineers are domain experts who
   happen to know ML, not the other way around.

5. **Leakage is Silent Poison**: If your model seems too good to be true, you
   have data leakage. The test set must simulate the real future.

6. **Regularization is Not Optional**: Every model with enough capacity will
   overfit. L2, dropout, early stopping — pick at least one.

7. **Understand Your Loss**: The loss function defines what "good" means.
   MSE ≠ MAE ≠ Huber ≠ Cross-Entropy. Each makes different tradeoffs about
   outliers, calibration, and robustness.
