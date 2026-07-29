# JAX — Functional ML Framework with Automatic Compilation & Parallelism

---

## 1. What JAX Is

```
JAX is Google's ML framework: NumPy on GPUs/TPUs with
automatic differentiation, JIT compilation, and auto-parallelism.

  The pitch in one line:
    "NumPy + autograd + jit + vmap + pmap, all composable."

  What makes JAX different from PyTorch:
    PyTorch: imperative, object-oriented, eager by default.
    JAX: FUNCTIONAL, transformation-based, compiled by default.

    In PyTorch: model = nn.Module with mutable state (.parameters()).
    In JAX: model = a pure function f(params, x) → y.
                    params are just arrays passed as arguments.
                    No hidden state. No side effects. Ever.

  Why functional?
    Pure functions are EASY TO COMPOSE with transformations:
      jit(f)     → compile f with XLA
      grad(f)    → differentiate f (automatic backward pass)
      vmap(f)    → vectorize f across a batch dimension
      pmap(f)    → parallelize f across devices

    These compose arbitrarily:
      jit(grad(vmap(f)))  → compiled batched gradient computation
    This composability is JAX's killer feature.

  Created by: Google Brain (2018), primarily Matthew Johnson,
              Roy Frostig, Dougal Maclaurin, Chris Leary.
  GitHub: https://github.com/jax-ml/jax

  Used by:
    - Google DeepMind (Gemini, AlphaFold 2, all TPU training)
    - Research labs wanting composable transforms
    - Increasingly production (Google-internal, some startups)

Timeline:
  2015  Autograd (predecessor, NumPy autodiff in Python)
  2018  JAX created at Google Brain
  2020  Google's large-scale training shifts from TF to JAX
  2021  DeepMind merges with Google Brain, adopts JAX
  2022  PaLM trained with JAX
  2023  Gemini trained with JAX on TPU v5
  2024+ JAX is Google's primary ML framework internally
```

---

## 2. The Core Transformations

```
JAX has four fundamental transformations. Everything is built on them.

  ┌──────────────────────────────────────────────────────────────┐
  │ 1. jit — Just-In-Time Compilation                          │
  │                                                              │
  │   @jax.jit                                                   │
  │   def f(x, w):                                               │
  │       return jax.nn.relu(x @ w)                              │
  │                                                              │
  │   First call: JAX traces f → captures computation graph     │
  │               → sends to XLA → compiles to GPU/TPU code     │
  │   Subsequent calls: runs compiled code directly              │
  │                                                              │
  │   How tracing works:                                         │
  │     JAX executes f with ABSTRACT values (shapes + dtypes,    │
  │     not actual numbers). Records every operation.            │
  │     Produces a Jaxpr (JAX expression) → StableHLO → XLA.   │
  │                                                              │
  │   Constraint: function must be PURE (no side effects).       │
  │   print() inside jit → only runs during tracing, not after. │
  │   Random state must be passed explicitly (PRNGKey).          │
  └──────────────────────────────────────────────────────────────┘

  ┌──────────────────────────────────────────────────────────────┐
  │ 2. grad — Automatic Differentiation                         │
  │                                                              │
  │   def loss_fn(params, x, y):                                │
  │       pred = model(params, x)                                │
  │       return jnp.mean((pred - y) ** 2)                      │
  │                                                              │
  │   grads = jax.grad(loss_fn)(params, x, y)                   │
  │   # grads has same tree structure as params                  │
  │   # Each leaf is ∂loss/∂param                               │
  │                                                              │
  │   How it works:                                              │
  │     JAX traces the function, builds a computation graph,     │
  │     applies reverse-mode autodiff on the graph.              │
  │     Same math as PyTorch's autograd, but on a traced graph   │
  │     rather than a dynamically-built tape.                    │
  │                                                              │
  │   Composability:                                              │
  │     jax.grad(jax.grad(f))   → second derivatives (Hessian)  │
  │     jax.jacobian(f)         → full Jacobian matrix           │
  │     jax.jvp(f, primals, tangents) → forward-mode AD          │
  │     These just work. In PyTorch, higher-order grads are hard.│
  └──────────────────────────────────────────────────────────────┘

  ┌──────────────────────────────────────────────────────────────┐
  │ 3. vmap — Automatic Vectorization (Batching)                │
  │                                                              │
  │   def predict_one(params, x):                                │
  │       return model(params, x)    # works on a single input   │
  │                                                              │
  │   # Automatically make it work on a batch:                   │
  │   predict_batch = jax.vmap(predict_one, in_axes=(None, 0))  │
  │   # in_axes=(None, 0) → don't batch params, batch x along 0│
  │                                                              │
  │   outputs = predict_batch(params, batch_x)                   │
  │   # Equivalent to: [predict_one(params, x) for x in batch_x]│
  │   # But compiled into efficient batched ops by XLA.          │
  │                                                              │
  │   Why this matters:                                          │
  │     Write code for ONE example. vmap makes it batched.       │
  │     No manual batch dimension management.                    │
  │     Especially useful for per-example gradients:             │
  │       per_example_grads = vmap(grad(loss_fn))(params, xs, ys)│
  │       → gradient of each example, no tricks needed.          │
  └──────────────────────────────────────────────────────────────┘

  ┌──────────────────────────────────────────────────────────────┐
  │ 4. pmap — Parallel Map Across Devices                       │
  │                                                              │
  │   @jax.pmap                                                  │
  │   def train_step(params, batch):                             │
  │       grads = jax.grad(loss_fn)(params, batch)               │
  │       grads = jax.lax.pmean(grads, axis_name='devices')     │
  │       return update(params, grads)                           │
  │                                                              │
  │   # Runs on ALL available GPUs/TPUs simultaneously.          │
  │   # pmean = AllReduce (average gradients across devices).   │
  │   # Each device gets a different batch shard.                │
  │                                                              │
  │   Note: pmap is being superseded by jax.sharding + jit      │
  │   for more flexible parallelism (see GSPMD section below).  │
  └──────────────────────────────────────────────────────────────┘

  The magic: ALL FOUR COMPOSE:

    jit(vmap(grad(f)))
    → compile(vectorize(differentiate(f)))
    → efficient batched gradient computation, compiled to GPU

    This is not possible in PyTorch. You'd need to manually
    write the batched gradient loop, then torch.compile it.
```

---

## 3. Functional Paradigm — Params as Data

```
In JAX, model parameters are NOT hidden inside objects.
They're just arrays (pytrees) you pass to functions.

  PyTorch style:
    model = nn.Linear(784, 256)          # params hidden inside
    y = model(x)                          # mutation, hidden state
    loss.backward()                       # mutates .grad fields
    optimizer.step()                      # mutates params in-place

  JAX style:
    params = init_fn(key, input_shape)    # params = explicit data
    y = apply_fn(params, x)              # pure function, no mutation
    grads = jax.grad(loss_fn)(params, x, y)  # grads = data
    params = jax.tree.map(                # new params (no mutation)
        lambda p, g: p - lr * g, params, grads)

  params is a PYTREE — a nested dict/tuple of arrays:
    params = {
        'layer1': {'weight': Array[784, 256], 'bias': Array[256]},
        'layer2': {'weight': Array[256, 10],  'bias': Array[10]},
    }

  JAX tree utilities operate on this structure:
    jax.tree.map(fn, tree)     → apply fn to every leaf
    jax.tree.leaves(tree)      → flatten to list of arrays
    jax.tree.structure(tree)   → the nesting structure

  Why functional matters for training:
    1. grad() needs a function to differentiate. No function → no grad.
    2. jit() traces a function. Mutable state breaks tracing.
    3. Parallelism: replicate params across devices = just copy the tree.
       No complex .module() state tracking like PyTorch.
    4. Checkpointing: save params = just serialize the pytree.
       No .state_dict() needed.
```

---

## 4. JAX Ecosystem — Libraries That Make It Usable

```
JAX itself is low-level (just arrays + transforms).
Libraries build the "framework" on top:

  ┌──────────────────────────────────────────────────────────────┐
  │ Library     │ What it provides                              │
  ├─────────────┼───────────────────────────────────────────────┤
  │ Flax        │ nn.Module equivalent for JAX (Google).        │
  │ (flax.linen)│ Define layers, manage params + state.         │
  │             │ Most popular JAX neural network library.      │
  ├─────────────┼───────────────────────────────────────────────┤
  │ Optax       │ Optimizers (Adam, SGD, schedules, clipping). │
  │             │ Composable: chain(clip_grads, adam, schedule).│
  ├─────────────┼───────────────────────────────────────────────┤
  │ Orbax       │ Checkpointing (save/load params, async save).│
  ├─────────────┼───────────────────────────────────────────────┤
  │ Equinox     │ Alternative to Flax. PyTorch-like feel but    │
  │             │ still pure-functional under the hood.         │
  ├─────────────┼───────────────────────────────────────────────┤
  │ Pax/Praxis  │ Google-internal training framework on JAX.    │
  │             │ Used for Gemini, PaLM, large-scale training.  │
  ├─────────────┼───────────────────────────────────────────────┤
  │ T5X/MaxText │ Reference implementations for LLM training    │
  │             │ on TPU pods. MaxText = modern reference.      │
  └─────────────┴───────────────────────────────────────────────┘

  A typical JAX training loop (with Flax + Optax):

    import jax
    import flax.linen as nn
    import optax

    class MLP(nn.Module):
        @nn.compact
        def __call__(self, x):
            x = nn.Dense(256)(x)
            x = nn.relu(x)
            x = nn.Dense(10)(x)
            return x

    model = MLP()
    params = model.init(jax.random.key(0), jnp.zeros((1, 784)))
    optimizer = optax.adam(1e-3)
    opt_state = optimizer.init(params)

    @jax.jit
    def train_step(params, opt_state, x, y):
        def loss_fn(params):
            pred = model.apply(params, x)
            return jnp.mean((pred - y) ** 2)
        grads = jax.grad(loss_fn)(params)
        updates, opt_state = optimizer.update(grads, opt_state)
        params = optax.apply_updates(params, updates)
        return params, opt_state

    for batch in dataloader:
        params, opt_state = train_step(params, opt_state, *batch)
```

---

## 5. Sharding & GSPMD — Automatic Multi-Device Parallelism

```
JAX's approach to distributed training:
  Don't write communication code. Annotate how data is sharded.
  XLA's GSPMD compiler figures out the rest.

  from jax.sharding import Mesh, NamedSharding, PartitionSpec as P

  # Define a mesh of devices
  devices = jax.devices()  # e.g., 8 TPUs
  mesh = Mesh(devices.reshape(2, 4), axis_names=('dp', 'tp'))
  #   dp=2 (data parallel replicas)
  #   tp=4 (tensor parallel shards)

  # Annotate how params are sharded:
  # Weight matrix: replicate along dp, shard columns along tp
  w_sharding = NamedSharding(mesh, P(None, 'tp'))
  # Input batch: shard along dp, replicate rest
  x_sharding = NamedSharding(mesh, P('dp', None))

  params = jax.device_put(params, w_sharding)
  x = jax.device_put(x, x_sharding)

  @jax.jit
  def forward(params, x):
      return x @ params    # XLA figures out the communication

  # XLA sees:
  #   x is dp-sharded, params is tp-sharded.
  #   Result needs AllReduce along tp axis.
  #   XLA inserts AllReduce AUTOMATICALLY.
  #   Developer writes ZERO communication code.

  ┌──────────────────────────────────────────────────────────────┐
  │ Comparison: manual vs automatic parallelism                 │
  │                                                              │
  │ Megatron-LM (PyTorch):                                      │
  │   - Hand-code column-parallel linear                         │
  │   - Hand-code row-parallel linear                            │
  │   - Manually insert AllReduce after row-parallel             │
  │   - Manually insert AllGather for sequence parallelism       │
  │   - 100s of lines of communication code per layer            │
  │   - Change strategy → rewrite the model                     │
  │                                                              │
  │ JAX + GSPMD:                                                │
  │   - Write the model as a normal function                     │
  │   - Annotate: P(None, 'tp') for tensor parallel              │
  │   - XLA inserts all communication                            │
  │   - Change strategy → change annotations only                │
  │   - 3-5 lines of sharding code, regardless of model size     │
  └──────────────────────────────────────────────────────────────┘

  This is how Google trains Gemini on thousands of TPUs.
  The model code doesn't contain communication logic.
  Sharding annotations + GSPMD handle everything.
```

---

## 6. JAX on TPUs — The Primary Target

```
JAX + XLA + TPU is Google's complete stack.
This combination doesn't exist anywhere else.

  Why TPUs:
    Google owns TPU hardware. No NVIDIA dependency.
    TPU v5e/v5p: 128×128 systolic array + 128 MB SRAM.
    High-bandwidth (ICI) interconnect between TPU chips.

  TPU topologies:
    TPU v5e pod:    256 chips, 2D torus topology
    TPU v5p pod:    8960 chips, 3D torus topology (huge!)
    TPU v6 (Trillium): next generation

  JAX sees TPUs as a mesh of devices:
    mesh = Mesh(jax.devices(), ('dp', 'fsdp', 'tp'))
    # 3D parallelism via sharding annotations alone.

  Performance characteristics:
    TPU v5e BF16:  197 TFLOPS per chip
    TPU v5p BF16:  459 TFLOPS per chip
    ICI bandwidth: 4.8 Tbps per chip (much higher than InfiniBand)
    HBM:           16-96 GB per chip (depends on variant)

  The ICI interconnect is key:
    AllReduce across 256 TPUs over ICI is ~10× faster than
    AllReduce across 256 GPUs over InfiniBand.
    This means TP doesn't need to be limited to 8 devices (NVLink).
    Can do TP=16 or higher on TPU pods.

  Downside: TPU is Google Cloud only.
    On-prem TPU: not available.
    If you leave Google Cloud, your JAX+TPU code needs porting.
    JAX works on GPU too, but the ecosystem is GPU-second.
```

---

## 7. JAX vs PyTorch

```
  ┌──────────────────────────────────────────────────────────────┐
  │                │ JAX                    │ PyTorch             │
  ├────────────────┼────────────────────────┼─────────────────────┤
  │ Paradigm       │ Functional (pure fns)  │ Imperative (objects)│
  │ Execution      │ Compiled (jit default) │ Eager (compile opt) │
  │ Autodiff       │ grad() transform       │ .backward() method  │
  │ Batching       │ vmap() transform       │ Manual batch dims   │
  │ Parallelism    │ Sharding + GSPMD (auto)│ DDP/FSDP (manual)   │
  │ State          │ Explicit (pytrees)     │ Hidden (nn.Module)  │
  │ Randomness     │ Explicit PRNGKey       │ Global RNG state    │
  │ GPU support    │ Good (via XLA)         │ Best (native CUDA)  │
  │ TPU support    │ Best (primary target)  │ Via torch-xla (okay)│
  │ Dynamic shapes │ Limited (XLA constraint)│ Native              │
  │ Debugging      │ Harder (traced graphs)  │ Easier (eager mode) │
  │ Ecosystem      │ Smaller (Flax, Optax)  │ Huge (HuggingFace+) │
  │ Industry use   │ Google, DeepMind       │ Everyone else       │
  │ Research papers│ Growing (~30%)          │ Dominant (~65%)     │
  ├────────────────┼────────────────────────┼─────────────────────┤
  │ Best for       │ TPU training, Google,  │ Everything else,    │
  │                │ research needing       │ GPU training/serving│
  │                │ composable transforms  │ widest ecosystem    │
  └────────────────┴────────────────────────┴─────────────────────┘

  When to choose JAX:
    - Training on Google Cloud TPUs (no real alternative)
    - Need higher-order derivatives (Hessians, Jacobians)
    - Need per-example gradients (vmap(grad(...)))
    - Want automatic parallelism (GSPMD vs manual Megatron)
    - Prefer functional programming style

  When to choose PyTorch:
    - Using NVIDIA GPUs (better ecosystem, CUDA-native)
    - Need dynamic shapes (variable seq lengths for inference)
    - Want largest library/model ecosystem (HuggingFace)
    - Team already knows PyTorch (most ML engineers do)
    - Debugging and rapid prototyping
```

---

## 8. Gotchas & Mental Model Shifts

```
Coming from PyTorch, these trip people up:

  1. NO IN-PLACE MUTATION
     x[0] = 5                  # ERROR in JAX
     x = x.at[0].set(5)       # creates a new array

     Why: pure functions can't mutate inputs.
     XLA needs to see the full data flow to optimize.

  2. EXPLICIT RANDOM STATE
     jax.random.normal(key, shape)     # must pass a PRNGKey
     key, subkey = jax.random.split(key)  # split for next use

     Why: pure functions can't access global RNG state.
     Deterministic + reproducible by construction.

  3. SHAPES MUST BE STATIC UNDER JIT
     @jax.jit
     def f(x):
         if x.shape[0] > 10:  # OK (shape is known at trace time)
         if x[0] > 5:         # ERROR (value not known at trace time)

     Why: XLA compiles for fixed shapes. Data-dependent control
     flow requires jax.lax.cond() or jax.lax.while_loop().

  4. TRACING vs EXECUTION
     @jax.jit
     def f(x):
         print("tracing!")    # prints ONCE (during tracing)
         return x + 1
     f(jnp.ones(3))           # prints "tracing!"
     f(jnp.ones(3))           # doesn't print (runs compiled code)

     Side effects happen during tracing, not execution.
     Use jax.debug.print() for runtime printing.

  5. PYTREES EVERYWHERE
     JAX transforms work on arbitrary nested structures,
     not just tensors. (params, opt_state, batch) are all pytrees.
     Everything flows through function arguments and return values.
```

---

## 9. Key Numbers

```
JAX + XLA performance:

  Compilation time (jit, first call):
    Small model:    ~2-5 seconds
    BERT-large:     ~30-60 seconds
    LLaMA-70B:     ~5-15 minutes (XLA whole-graph compile)
    Cached: <1 second after first compile

  Training throughput (vs PyTorch):
    On GPU: roughly comparable (±10%)
      JAX wins on fusion-heavy models (more aggressive XLA fusion)
      PyTorch wins on dynamic workloads (no recompilation)
    On TPU: JAX is ~1.3-1.5× faster than PyTorch (torch-xla)
      Native XLA path vs torch-xla bridge layer

  Google-scale training:
    PaLM 540B: trained on 6144 TPU v4 chips with JAX
    Gemini: trained on TPU v5p pods with JAX
    Time: weeks to months, MFU ~50-60% on TPU

  GSPMD overhead:
    Sharding annotation → XLA auto-communication insertion
    ~0-5% overhead vs hand-written communication
    (XLA is very good at choosing optimal collectives)

  vmap speedup over manual loop:
    Per-example gradients: 10-50× faster than Python loop
    (vmap fuses into batched ops, loop doesn't)
```
