# DeepSpeed — Memory-Efficient Distributed Training

---

## 1. What DeepSpeed Does

```
DeepSpeed (Microsoft) solves a different problem than Megatron.

  Megatron: split the MODEL across GPUs (tensor/pipeline parallelism).
  DeepSpeed: split the OPTIMIZER STATE across GPUs (ZeRO).

  Why optimizer state matters:
    Model weights (BF16):           2 bytes × N params
    Gradients (BF16):               2 bytes × N
    Adam optimizer momentum (FP32): 4 bytes × N
    Adam optimizer variance (FP32): 4 bytes × N
    Master weights (FP32):          4 bytes × N    (for accumulation)
    ───────────────────────────────────────────────
    Total:                          16 bytes × N

    70B model: 16 × 70B = 1.12 TB total memory needed.
    With 8 GPUs (80 GB each): 640 GB. NOT enough for 1.12 TB.

    But note: weights are only 140 GB. The REST is optimizer state.
    What if we DON'T replicate optimizer state on every GPU?

  That's ZeRO: Zero Redundancy Optimizer.
```

---

## 2. ZeRO Stages — Progressive Memory Savings

```
Standard Data Parallel (no ZeRO):
  Every GPU stores: weights + gradients + optimizer states.
  All identical. 100% redundancy.
  Memory per GPU: 16 bytes × N params.

  8 GPUs, 70B params:
    Each GPU: 16 × 70B = 1.12 TB. Doesn't fit in 80 GB.


ZERO STAGE 1: Partition OPTIMIZER STATES
─────────────────────────────────────────
  Optimizer states (momentum + variance + master weights) are sharded.
  Each GPU stores 1/Nth of the optimizer states.

  Every GPU still has:  full weights, full gradients.
  But optimizer states: only 1/8th.

  Memory per GPU:
    Weights:          140 GB (full copy)
    Gradients:        140 GB (full copy)
    Optimizer (1/8):  105 GB / 8 = ~13 GB
    Total:            ~293 GB → still doesn't fit.

  After optimizer step, each GPU has updated weights only for its
  shard. AllGather to distribute updated weights to all GPUs.


ZERO STAGE 2: Partition GRADIENTS too
──────────────────────────────────────
  Gradients are also sharded. Each GPU accumulates only its shard.

  Each GPU has:  full weights, 1/8 gradients, 1/8 optimizer states.

  Memory per GPU:
    Weights:          140 GB (full copy)
    Gradients (1/8):  ~17.5 GB
    Optimizer (1/8):  ~13 GB
    Total:            ~170 GB → closer but still too big.

  During backward: gradients are ReduceScatter'd as they're produced.
  Each GPU ends up with 1/8 of the fully-reduced gradients.


ZERO STAGE 3: Partition EVERYTHING (weights too)
──────────────────────────────────────────────────
  Weights are ALSO sharded. No GPU has the full model.

  Each GPU has:  1/8 weights, 1/8 gradients, 1/8 optimizer states.

  Memory per GPU:
    Weights (1/8):    ~17.5 GB
    Gradients (1/8):  ~17.5 GB
    Optimizer (1/8):  ~13 GB
    Total:            ~48 GB → FITS on 80 GB GPU! ✓

  But: to compute a layer's forward pass, you need the FULL weights.
  Solution: AllGather weights just-in-time, compute, then discard.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Forward pass for layer L:                                  │
  │    1. AllGather: assemble full weights of layer L           │
  │       (each GPU contributes its 1/8, now all have full)     │
  │    2. Compute: forward(input, full_weights)                 │
  │    3. Discard: free the full weights (back to 1/8 only)    │
  │    4. Move to layer L+1                                     │
  │                                                              │
  │  Backward pass for layer L:                                 │
  │    1. AllGather: assemble full weights again                │
  │    2. Compute: backward(grad_output, full_weights)          │
  │    3. ReduceScatter: each GPU gets its 1/8 of gradient     │
  │    4. Discard full weights                                  │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

  Communication cost: AllGather + ReduceScatter for EVERY layer.
  This is 1.5× the communication of standard Data Parallel.
  Tradeoff: 8× less memory for 1.5× more communication.

  ┌────────────────┬────────────┬──────────────┬──────────────────┐
  │ ZeRO Stage     │ Memory/GPU │ Communication│ What's sharded   │
  ├────────────────┼────────────┼──────────────┼──────────────────┤
  │ No ZeRO (DDP)  │ 16N bytes  │ 1× (AllReduce│ Nothing          │
  │                │            │ gradients)   │                  │
  │ ZeRO-1         │ ~10N bytes │ 1× + AllGather│ Optimizer states │
  │ ZeRO-2         │ ~6N bytes  │ 1× + smaller │ + Gradients      │
  │ ZeRO-3         │ ~2N bytes  │ 1.5× per layer│ + Weights       │
  └────────────────┴────────────┴──────────────┴──────────────────┘
```

---

## 3. ZeRO-Offload — Use CPU Memory and NVMe

```
Even ZeRO-3 might not be enough for very large models on few GPUs.

  ZeRO-Offload: move optimizer states to CPU memory.
  ZeRO-Infinity: move to NVMe SSD (essentially unlimited memory).

  ┌────────────────────────────────────────────────────────────┐
  │                                                            │
  │  GPU (80 GB):                                             │
  │    Current layer's weights (full, via AllGather)           │
  │    Current layer's activations                             │
  │    Current layer's gradients                               │
  │                                                            │
  │  CPU (2 TB RAM):                                           │
  │    All optimizer states (momentum, variance)               │
  │    Master weights (FP32)                                   │
  │    Gradients waiting for optimizer step                    │
  │                                                            │
  │  NVMe (30 TB):                                             │
  │    Overflow from CPU (ZeRO-Infinity)                      │
  │    Checkpoints                                             │
  │                                                            │
  │  Flow:                                                     │
  │    Forward: AllGather weights to GPU → compute             │
  │    Backward: compute gradients → offload to CPU            │
  │    Optimizer step: CPU computes Adam update (slow but has  │
  │      2TB RAM). Upload updated weights back to GPU.        │
  │                                                            │
  └────────────────────────────────────────────────────────────┘

  Tradeoff:
    Pro: can train 1T models on 8 GPUs (with CPU offloading).
    Con: PCIe bandwidth (32 GB/s) is 28× slower than NVLink.
         GPU often stalls waiting for weights from CPU.
         Only worth it when you DON'T have enough GPUs.

  In practice:
    Large labs (Meta, OpenAI): enough GPUs. Use ZeRO-1 or ZeRO-2.
    Smaller teams (researchers, startups): ZeRO-3 + offload.
```

---

## 4. DeepSpeed vs FSDP vs Megatron

```
All three do distributed training. Different approaches:

  ┌────────────────┬────────────────┬────────────────┬────────────────┐
  │                │ DeepSpeed ZeRO │ PyTorch FSDP   │ Megatron-LM    │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ Core idea      │ Shard optimizer│ Shard params   │ Split model    │
  │                │ states, grads, │ (like ZeRO-3)  │ with TP + PP   │
  │                │ weights        │                │                │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ Maintained by  │ Microsoft      │ PyTorch/Meta   │ NVIDIA         │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ Integration    │ Wrapper around │ Native PyTorch │ Custom codebase│
  │                │ PyTorch        │ (torch.dist)   │ (not modular)  │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ TP support     │ No (use with   │ No (use with   │ YES (core      │
  │                │ Megatron)      │ Megatron)      │ feature)       │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ PP support     │ Yes (basic)    │ No             │ YES (1F1B,     │
  │                │                │                │ interleaved)   │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ CPU offload    │ YES            │ YES            │ No             │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ Ease of use    │ Config file    │ Wrap model     │ Must use their │
  │                │ (JSON)         │ (1-2 lines)    │ training loop  │
  ├────────────────┼────────────────┼────────────────┼────────────────┤
  │ Best for       │ <100B models   │ <100B models   │ >100B models   │
  │                │ on fewer GPUs  │ PyTorch-native │ at full scale  │
  └────────────────┴────────────────┴────────────────┴────────────────┘

In practice, labs COMBINE them:

  Megatron-DeepSpeed (common combo):
    Megatron handles TP + PP (model splitting).
    DeepSpeed handles ZeRO (memory optimization for DP).
    Best of both worlds.

  Meta's approach for LLaMA-3:
    FSDP (ZeRO-3 equivalent) for data parallel.
    Custom TP implementation (Megatron-inspired).
    Custom PP implementation.
    All in PyTorch-native code.
```

---

## 5. DeepSpeed Configuration

```json
// deepspeed_config.json — ZeRO Stage 2 example
{
  "train_batch_size": 2048,
  "gradient_accumulation_steps": 8,

  "fp16": {
    "enabled": true,
    "loss_scale": 0,           // dynamic loss scaling
    "initial_scale_power": 16
  },

  "zero_optimization": {
    "stage": 2,                 // ZeRO stage (1, 2, or 3)
    "allgather_partitions": true,
    "reduce_scatter": true,
    "overlap_comm": true,       // overlap with backward
    "contiguous_gradients": true
  },

  "gradient_clipping": 1.0,

  "optimizer": {
    "type": "AdamW",
    "params": {
      "lr": 3e-4,
      "betas": [0.9, 0.95],
      "weight_decay": 0.1
    }
  },

  "scheduler": {
    "type": "WarmupDecayLR",
    "params": {
      "warmup_min_lr": 0,
      "warmup_max_lr": 3e-4,
      "warmup_num_steps": 2000,
      "total_num_steps": 100000
    }
  }
}
```

```python
# Training code with DeepSpeed — minimal changes to existing PyTorch code
import deepspeed

model = TransformerModel(config)
optimizer = None  # DeepSpeed creates optimizer from config

model_engine, optimizer, _, _ = deepspeed.initialize(
    model=model,
    model_parameters=model.parameters(),
    config="deepspeed_config.json"
)

for batch in dataloader:
    loss = model_engine(batch)
    model_engine.backward(loss)
    model_engine.step()
    # DeepSpeed handles: ZeRO sharding, communication,
    # mixed precision, gradient clipping — all automatic.
```

---

## 6. Other DeepSpeed Features

```
Beyond ZeRO, DeepSpeed provides:

  1. MIXED PRECISION TRAINING
     Automatic FP16/BF16 with loss scaling.
     FP8 support for H100.

  2. GRADIENT CHECKPOINTING
     DeepSpeed's implementation with CPU offloading.
     Checkpointed activations stored on CPU instead of GPU.

  3. SPARSE ATTENTION
     DeepSpeed Sparse Attention for long sequences.
     Reduces attention from O(n²) to O(n√n).
     (Flash Attention largely replaced this.)

  4. 1-BIT ADAM
     Compress gradients to 1 bit before AllReduce.
     ~5× less communication. Small accuracy tradeoff.
     Useful when network bandwidth is the bottleneck.

  5. MoE (Mixture of Experts) SUPPORT
     Parallelism strategies for MoE models.
     Expert parallelism: different GPUs host different experts.
     Used by DeepSeek for V3/R1 training.

  6. INFERENCE
     DeepSpeed-Inference: optimized transformer kernels.
     Tensor parallelism for inference.
     Largely superseded by vLLM/TGI for LLM serving.
```

---

## 7. When to Use What

```
Decision tree:

  Model fits on one GPU (< 80 GB)?
    YES → Standard PyTorch DDP. No DeepSpeed or Megatron needed.

  Model fits with ZeRO-1 or ZeRO-2?
    YES → DeepSpeed ZeRO-1/2 or PyTorch FSDP.
          Minimal code changes. Good performance.

  Model needs ZeRO-3 (weights sharded)?
    Few GPUs (<64): → DeepSpeed ZeRO-3 (maybe + offload).
    Many GPUs (64+): → Megatron TP + DeepSpeed ZeRO-2 for DP.

  Model > 100B parameters?
    → Full 3D parallelism: Megatron (TP+PP) + DeepSpeed (ZeRO for DP).
    → Or Meta's approach: Megatron-style TP+PP + FSDP for DP.

  Only 1-2 GPUs but want to train 70B?
    → DeepSpeed ZeRO-3 + CPU offloading.
    → Very slow but it works. Good for research/fine-tuning.

  ┌─────────────────┬────────────────────────────────────────┐
  │ Scenario        │ Recommendation                         │
  ├─────────────────┼────────────────────────────────────────┤
  │ 7B on 1 GPU     │ Standard PyTorch, maybe QLoRA          │
  │ 7B on 8 GPUs    │ PyTorch DDP (simplest)                 │
  │ 70B on 8 GPUs   │ DeepSpeed ZeRO-3                       │
  │ 70B on 64 GPUs  │ ZeRO-2 + TP=8 (or FSDP + TP)         │
  │ 405B on 1K GPUs │ Megatron TP+PP + ZeRO/FSDP for DP     │
  │ 405B on 16K GPU │ Full 3D parallelism (Meta/NVIDIA style)│
  └─────────────────┴────────────────────────────────────────┘
```
