# Training Math — Forward, Backward, Gradients, Loss, Optimizer

---

## 1. The Whole Point of Training

```
You have a function f(x) that makes predictions.
f has knobs (weights) you can turn.
You want to turn the knobs so predictions match reality.

  Input: x = 2.0  (e.g., house size in 1000 sqft)
  True answer: y = 7.0  (e.g., price in $100K)

  Your model: f(x) = w * x + b
  Current knobs: w = 1.0, b = 0.0

  Prediction: f(2.0) = 1.0 * 2.0 + 0.0 = 2.0
  Reality: 7.0

  We're way off. How do we fix w and b?
```

---

## 2. Forward Pass — "How Wrong Am I?"

```
Forward = plug in numbers, get a prediction, measure error.

  x = 2.0, w = 1.0, b = 0.0

  Step by step:
    z = w * x        = 1.0 * 2.0 = 2.0
    y_pred = z + b   = 2.0 + 0.0 = 2.0

  How wrong? Use LOSS function (mean squared error):
    loss = (y_pred - y_true)²
         = (2.0 - 7.0)²
         = (-5.0)²
         = 25.0

  Loss = 25.0. That's our "wrongness score."
  Goal: make loss as close to 0 as possible.
```

---

## 3. Backward Pass — "Which Direction Should I Turn Each Knob?"

```
This is where calculus comes in. But the intuition is simple:

  "If I increase w by a tiny bit, does the loss go UP or DOWN?"
  "If I increase b by a tiny bit, does the loss go UP or DOWN?"

That's all a gradient is:
  ∂loss/∂w = "how much does loss change when I nudge w?"
  ∂loss/∂b = "how much does loss change when I nudge b?"

If ∂loss/∂w is POSITIVE → increasing w makes loss WORSE → decrease w.
If ∂loss/∂w is NEGATIVE → increasing w makes loss BETTER → increase w.

Let's compute it with the CHAIN RULE.
Backward = walk backwards through the computation, one step at a time.
```

```
Forward was:
  z = w * x           (step 1)
  y_pred = z + b      (step 2)
  loss = (y_pred - y)² (step 3)

Backward walks in REVERSE order:

  ─── Step 3 backward: loss = (y_pred - y)² ───

    ∂loss/∂y_pred = 2 * (y_pred - y)
                  = 2 * (2.0 - 7.0)
                  = 2 * (-5.0)
                  = -10.0

    "If y_pred goes up by 1, loss goes down by 10."
    This makes sense: y_pred = 2.0 is BELOW the target 7.0,
    so increasing y_pred would help (loss decreases).


  ─── Step 2 backward: y_pred = z + b ───

    ∂y_pred/∂z = 1      (if z goes up by 1, y_pred goes up by 1)
    ∂y_pred/∂b = 1      (if b goes up by 1, y_pred goes up by 1)

    Chain rule:
      ∂loss/∂z = ∂loss/∂y_pred × ∂y_pred/∂z = -10.0 × 1 = -10.0
      ∂loss/∂b = ∂loss/∂y_pred × ∂y_pred/∂b = -10.0 × 1 = -10.0

    "If b goes up by 1, loss goes down by 10."
    → ∂loss/∂b = -10.0  ← this is b's gradient!


  ─── Step 1 backward: z = w * x ───

    ∂z/∂w = x = 2.0     (if w goes up by 1, z goes up by x = 2.0)

    Chain rule:
      ∂loss/∂w = ∂loss/∂z × ∂z/∂w = -10.0 × 2.0 = -20.0

    "If w goes up by 1, loss goes down by 20."
    → ∂loss/∂w = -20.0  ← this is w's gradient!


SUMMARY of backward:
  ∂loss/∂w = -20.0    (w's gradient)
  ∂loss/∂b = -10.0    (b's gradient)

Both are NEGATIVE → increasing both w and b would DECREASE the loss.
That's the direction we want to go!
```

---

## 4. Optimizer — "Turn the Knobs"

```
The simplest optimizer: GRADIENT DESCENT.

  new_w = old_w - learning_rate × gradient
  new_b = old_b - learning_rate × gradient

  Why SUBTRACT? Because:
    gradient > 0 → increasing param makes loss WORSE → go DOWN
    gradient < 0 → increasing param makes loss BETTER → go UP
    Subtracting flips the sign → always moves toward lower loss.

  With learning_rate = 0.01:
    w_new = 1.0 - 0.01 × (-20.0) = 1.0 + 0.2 = 1.2
    b_new = 0.0 - 0.01 × (-10.0) = 0.0 + 0.1 = 0.1

  Old prediction: 1.0 * 2.0 + 0.0 = 2.0   (loss = 25.0)
  New prediction: 1.2 * 2.0 + 0.1 = 2.5   (loss = (2.5-7)² = 20.25)

  Loss went from 25.0 → 20.25. We improved!
  Repeat this 100 times → w → 3.0, b → 1.0, f(2.0) = 7.0. Perfect.
```

---

## 5. Why Learning Rate Matters

```
Learning rate = how big a step you take.

  Too big (lr = 1.0):
    w = 1.0 - 1.0 × (-20.0) = 21.0
    Prediction: 21.0 * 2.0 + 0.0 = 42.0
    Loss: (42.0 - 7.0)² = 1225.0   ← WORSE! Overshot!

  Too small (lr = 0.0001):
    w = 1.0 - 0.0001 × (-20.0) = 1.002
    Prediction: 1.002 * 2.0 = 2.004
    Loss: 24.96   ← barely changed. Will take forever.

  Just right (lr = 0.01):
    Makes steady progress without overshooting.

  ┌─────────────────────────────────────────────────┐
  │ loss                                            │
  │  │                                              │
  │  │   lr too big: ╱╲  ╱╲  ╱╲  (bouncing)       │
  │  │              ╱  ╲╱  ╲╱  ╲                   │
  │  │                                              │
  │  │  ╲                                           │
  │  │   ╲  lr just right (smooth descent)         │
  │  │    ╲                                         │
  │  │     ╲___________                             │
  │  │                                              │
  │  │  ╲                                           │
  │  │   ╲ lr too small (barely moving)            │
  │  │    ╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲╲                     │
  │  └──────────────────────────── training steps   │
  └─────────────────────────────────────────────────┘
```

---

## 6. Adam — The Optimizer Everyone Uses

```
Plain gradient descent has problems:
  - Same learning rate for ALL parameters
  - Doesn't adapt to gradient history
  - Gets stuck in flat regions, overshoots in steep regions

Adam (Adaptive Moment Estimation) fixes this:

  For EACH parameter, Adam tracks:
    m = running average of gradients      (momentum — "which direction?")
    v = running average of gradients²     (variance — "how bumpy?")

  Update rule:
    m = 0.9 × m_old + 0.1 × gradient            (smooth out direction)
    v = 0.999 × v_old + 0.001 × gradient²        (track bumpiness)
    param -= lr × m / (√v + ε)                    (adapt step size)

  WHY this works:
    If gradients consistently point the same way (m is large):
      → take bigger steps (momentum carries you)

    If gradients are wild and noisy (v is large):
      → take SMALLER steps (√v in denominator shrinks the update)
      → automatic per-parameter learning rate

  This is why Adam needs 2× extra memory:
    For each parameter, store m and v.
    70B params × 4 bytes × 2 (m + v) = 560 GB of optimizer state.

  ε (epsilon, typically 1e-8):
    Prevents division by zero when v ≈ 0.
```

---

## 7. A Real Neural Network — Multiple Layers

```
The same process, just MORE chain rule steps.

  x ──► [W1 × x + b1] ──► relu ──► [W2 × h + b2] ──► loss
        layer 1              ↑       layer 2
                          activation
                          function

  Forward:
    h = W1 @ x + b1         (hidden layer output)
    a = relu(h)              (activation: max(0, h))
    y_pred = W2 @ a + b2    (output)
    loss = (y_pred - y)²

  Backward (reverse order):
    ∂loss/∂y_pred = 2(y_pred - y)

    ∂loss/∂W2 = ∂loss/∂y_pred × a.T       ← gradient for layer 2 weights
    ∂loss/∂b2 = ∂loss/∂y_pred              ← gradient for layer 2 bias

    ∂loss/∂a = W2.T × ∂loss/∂y_pred       ← gradient flowing backward

    ∂loss/∂h = ∂loss/∂a × relu'(h)        ← relu'(h) = 1 if h>0, else 0

    ∂loss/∂W1 = ∂loss/∂h × x.T            ← gradient for layer 1 weights
    ∂loss/∂b1 = ∂loss/∂h                   ← gradient for layer 1 bias

  Each layer's backward is just: "how does MY output affect the loss?"
  Chain rule connects them all: multiply local gradients along the path.

  80-layer transformer: same idea, just 80 links in the chain.
  PyTorch's autograd does this automatically for you.
```

---

## 8. Putting It All Together — The Training Loop

```python
# This is the ENTIRE training algorithm:

for epoch in range(100):
    for batch in dataloader:
        x, y = batch

        # 1. FORWARD: compute prediction + loss
        y_pred = model(x)                    # builds computation graph
        loss = loss_fn(y_pred, y)            # measures wrongness

        # 2. BACKWARD: compute gradients
        optimizer.zero_grad()                # clear old gradients
        loss.backward()                      # walk graph backwards,
                                             # fill param.grad for every param

        # 3. OPTIMIZE: update weights
        optimizer.step()                     # param -= lr * param.grad
                                             # (Adam: with momentum + adaptive lr)

# That's it. Every ML model trains this way.
# The only things that change are:
#   - model architecture (what f looks like)
#   - loss function (how you measure wrongness)
#   - optimizer (how you update weights)
#   - learning rate schedule (how lr changes over time)
```

```
The cycle visualized:

  ┌──────────────────────────────────────────────────┐
  │                                                  │
  │         ┌─────────────────────────┐              │
  │         │                         │              │
  │    ┌────▼────┐   ┌──────────┐   ┌┴───────────┐  │
  │    │ FORWARD │──►│ BACKWARD │──►│ OPTIMIZER  │  │
  │    │         │   │          │   │   STEP     │  │
  │    │ data →  │   │ loss →   │   │ gradients →│  │
  │    │ predict │   │ gradients│   │ update     │  │
  │    │ → loss  │   │ for each │   │ weights    │  │
  │    │         │   │ weight   │   │            │  │
  │    └─────────┘   └──────────┘   └────────────┘  │
  │         ▲                              │         │
  │         │          repeat              │         │
  │         └──────────────────────────────┘         │
  │                                                  │
  │    Loss decreases each iteration (hopefully).   │
  │    Stop when loss is low enough or validation    │
  │    accuracy stops improving.                     │
  │                                                  │
  └──────────────────────────────────────────────────┘
```
