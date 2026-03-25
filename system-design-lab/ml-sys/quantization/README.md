# Quantization — How to Shrink Models 2-4x

## What It Is

Quantization reduces the precision of model weights from fp16 (2 bytes) to int8 (1 byte) or int4 (0.5 bytes). Less memory, faster inference, minimal quality loss.

## Why It Matters

```
LLaMA-70B serving:
  FP16: 70B × 2 bytes = 140 GB → needs 2× A100 80GB ($6/hour)
  INT8: 70B × 1 byte  = 70 GB  → needs 1× A100 80GB ($3/hour)  ← 50% cost
  INT4: 70B × 0.5 bytes = 35GB → fits on 1× A100 40GB ($1.5/hr) ← 75% cost

LLM inference is MEMORY-BOUND (loading weights is the bottleneck).
Smaller weights → less memory to move→ faster inference.
INT8: ~2x faster, <1% quality loss
INT4: ~3x faster, ~2-3% quality loss
```

## How Quantization Works

```
Symmetric quantization (INT8):
  Map float range [-max, +max] to [-127, +127]

  scale = max(abs(weights)) / 127
  quantized = round(weight / scale)
  dequantized = quantized × scale

  Example:
    weights = [0.5, -0.3, 1.2, -0.8]
    max = 1.2
    scale = 1.2 / 127 = 0.00945
    quantized = [53, -32, 127, -85]      (int8 values)
    dequantized = [0.500, -0.302, 1.200, -0.803]
    error = [0.000, -0.002, 0.000, -0.003]  (very small!)

Per-tensor vs per-channel:
  Per-tensor: one scale for the entire weight matrix (fast, less accurate)
  Per-channel: one scale per output channel (slower, more accurate)
  Per-group: one scale per group of 128 values (best quality, used in GPTQ)
```

## Weight-Only Quantization (Post-Training)

```
GPTQ (Generalized Post-Training Quantization):
  - Quantize weights to 4-bit
  - Use calibration data (~128 samples) to minimize output error
  - Groups of 128 weights share one FP16 scale factor
  - Quality: ~1-2% loss on most benchmarks
  - Memory: 4x reduction

AWQ (Activation-Aware Weight Quantization):
  - Observe that some weights are more important than others
  - Keep important weights at higher precision
  - "Important" = weights that correspond to large activations
  - Slightly better quality than GPTQ at same bit-width

BitsAndBytes (easy to use):
  - NF4 quantization (QLoRA paper)
  - Double quantization (quantize the scales too)
  - Simple API: model = AutoModel.from_pretrained(..., load_in_4bit=True)
```

## Activation Quantization (W8A8)

```
Weight-only: weights are int8, activations are still fp16
  Matrix multiply: dequantize weight → fp16 matmul → fp16 output
  Benefits: smaller model, but matmul is still fp16

W8A8: BOTH weights AND activations are int8
  Matrix multiply: int8 × int8 → int32 accumulator → scale → fp16 output
  Benefits: can use INT8 Tensor Cores (2x throughput on A100/H100!)

The challenge: activations have OUTLIERS
  Most activations: small values [-1, 1]
  Outliers: a few values can be 100x larger
  If you quantize naively: outliers eat the entire int8 range,
  all small values get quantized to 0 → catastrophic quality loss

SmoothQuant solution:
  Migrate the quantization difficulty from activations to weights:
    Y = (X × diag(s)) × (diag(s)^(-1) × W)
  Choose s to make activations smoother (smaller range)
  while making weights slightly harder to quantize.
  Net result: both are easy to quantize → W8A8 with good quality.
```

## FP8 Quantization (H100/H200)

```
FP8: 1 sign + 4 exponent + 3 mantissa (E4M3)
  or  1 sign + 5 exponent + 2 mantissa (E5M2)

Unlike INT8 (uniform spacing), FP8 has LOG spacing:
  More precision near 0 (where most values are)
  Less precision for large values (rare)

FP8 on H100: 2x throughput over FP16 using FP8 Tensor Cores
  - Nearly transparent: just change input dtype
  - Quality: comparable to FP16 for inference, usable for training
  - Training: forward in FP8 (E4M3), backward in FP8 (E5M2)
```

## Quantization Summary

```
Method      Bits  Memory   Speed   Quality   Ease of use
─────────────────────────────────────────────────────────
FP16        16    1x       1x      baseline  default
BF16        16    1x       1x      ~same     PyTorch default
FP8         8     2x       ~2x     ~same     H100+ only
INT8 W-only 8     2x       ~1.5x   ~same     bitsandbytes
INT8 W8A8   8     2x       ~2x     <1% loss  SmoothQuant
INT4 GPTQ   4     4x       ~3x     1-2% loss GPTQ library
INT4 AWQ    4     4x       ~3x     <1% loss  AWQ library
NF4 QLoRA   4     4x       N/A     training  bitsandbytes
INT2 (exp)  2     8x       ~5x     5-10%     research
```
