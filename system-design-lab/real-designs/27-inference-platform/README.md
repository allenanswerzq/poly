# Design an Inference Platform (like Triton / SageMaker Endpoints / vLLM serving)

## Problem Statement

Design a platform that serves ML models in production with low latency, high throughput, and cost efficiency. The platform handles model deployment, autoscaling, batching, and multi-model serving.

## Requirements

### Functional
- Deploy a model (upload weights, configure hardware)
- Serve predictions via REST/gRPC API
- Autoscale based on traffic (scale to zero when idle)
- Support multiple models on the same GPU (multi-model serving)
- A/B testing (route % of traffic to model version B)
- Canary deployment (gradual rollout of new model version)

### Non-Functional
- Latency: P50 < 50ms, P99 < 200ms (for interactive use)
- Throughput: 10K+ requests/sec per model
- GPU utilization: >70% (GPU hours cost $2-4/hour)
- Availability: 99.9% (model must be serving)
- Scale to zero: don't pay for idle models

## High-Level Architecture

```
┌──────────────┐     ┌──────────────────┐     ┌─────────────────────┐
│   Clients     │────►│   API Gateway     │────►│   Router / LB        │
│  (REST/gRPC) │     │  (auth, rate limit)│     │  (model routing,     │
└──────────────┘     └──────────────────┘     │   A/B split)         │
                                               └──────────┬──────────┘
                                                          │
                              ┌────────────────┬──────────┴──────────┐
                              │                │                     │
                         ┌────▼─────┐    ┌─────▼────┐    ┌─────────▼──┐
                         │ Model A  │    │ Model B  │    │ Model C    │
                         │ (LLM)    │    │ (Recsys) │    │ (Vision)   │
                         │ 4×A100   │    │ 1×T4     │    │ 1×T4       │
                         │ vLLM     │    │ Triton   │    │ TorchServe │
                         └──────────┘    └──────────┘    └────────────┘
                              │
                              │ autoscaler watches queue depth
                              ▼
                    ┌──────────────────────┐
                    │   Autoscaler          │
                    │   (scale replicas     │
                    │    based on RPS/      │
                    │    queue depth/GPU%)  │
                    └──────────────────────┘
```

## Key Components

### 1. Model Server — The Core Serving Engine

```
Request lifecycle:
  1. Receive request (HTTP/gRPC)
  2. Preprocess (tokenize, resize image, normalize)
  3. Batch with other requests (dynamic batching)
  4. GPU inference (the actual model forward pass)
  5. Postprocess (decode tokens, format output)
  6. Return response

Dynamic batching — THE key optimization:
  ┌─────────────────────────────────────────────┐
  │ Without batching:                            │
  │   Req 1 → GPU → response  (GPU 10% utilized)│
  │   Req 2 → GPU → response  (GPU 10% utilized)│
  │                                              │
  │ With dynamic batching (wait 5ms):            │
  │   Req 1 ─┐                                  │
  │   Req 2 ─┼→ batch[1,2,3,4] → GPU → responses│
  │   Req 3 ─┤   (GPU 80% utilized!)            │
  │   Req 4 ─┘                                  │
  │                                              │
  │ Tradeoff: +5ms latency, 8x throughput        │
  └─────────────────────────────────────────────┘
```

### 2. Autoscaler

```
Metrics to scale on:
  - Request queue depth (reactive: scale when queue grows)
  - GPU utilization (proactive: scale before saturation)
  - Requests per second (predictive: based on traffic patterns)
  - P99 latency (quality: scale when latency degrades)

Scale-to-zero:
  No requests for 10 min → scale to 0 replicas → no GPU cost
  First request after idle → cold start (~30-60s to load model)
  Solution: keep 1 replica warm for popular models

Scaling math:
  Target: P99 < 200ms, each replica handles 50 RPS
  Traffic: 300 RPS
  Replicas needed: ceil(300/50) = 6 replicas
  + 1 buffer replica for spikes = 7 replicas
```

### 3. Model Registry + Deployment Pipeline

```
Model lifecycle:
  1. Train → save to model registry (S3 + metadata DB)
  2. Validate → run eval suite, check accuracy threshold
  3. Stage → deploy to staging, run integration tests
  4. Canary → 5% of prod traffic to new version
  5. Promote → 100% traffic to new version
  6. Rollback → instant switch back if metrics degrade

Blue-green deployment:
  Version A (current): serving 100% traffic
  Version B (new):     loaded on standby GPUs
  Switch: update router to point to B (instant, no downtime)
```

### 4. Multi-Model Serving (GPU Sharing)

```
Problem: small models don't need a full GPU
  Sentiment classifier: 100MB model, uses 2% of A100
  Paying for 100% of a $2/hour GPU for 2% utilization = waste

Solutions:
  1. MIG (Multi-Instance GPU):
     Split A100 into 7 slices, run 7 small models
     Each slice: isolated memory + compute

  2. Temporal multiplexing:
     Load model A → inference → unload
     Load model B → inference → unload
     Good if models are small enough to swap quickly

  3. Pack on same GPU:
     Model A uses 2GB VRAM, Model B uses 3GB VRAM
     Both fit on 1 GPU (80GB), run concurrently
     Risk: memory contention
```

### 5. Optimization Techniques

```
Quantization:
  FP32 (4 bytes) → INT8 (1 byte) → INT4 (0.5 bytes)
  4x less memory, 2-4x faster inference
  Accuracy drop: <1% for INT8, ~2-3% for INT4

KV Cache optimization (LLMs):
  PagedAttention (vLLM): 2-4x more concurrent requests
  Prefix caching: reuse KV cache for shared system prompts

Speculative decoding (LLMs):
  Small model drafts N tokens, large model verifies in parallel
  2-3x faster generation for well-matched draft/target models

Continuous batching (LLMs):
  Don't wait for longest sequence to finish
  Add new requests as others complete
  GPU stays at ~100% utilization
```

## LLM-Specific Serving Architecture

```
┌────────────────────────────────────────────────────────────┐
│                    LLM Serving (vLLM/SGLang)                │
│                                                             │
│  ┌───────────────┐   ┌──────────────┐   ┌───────────────┐ │
│  │ Request Queue  │──►│  Scheduler    │──►│ Model Executor │ │
│  │ (priority +   │   │  (continuous  │   │ (tensor       │ │
│  │  rate limit)  │   │   batching)   │   │  parallel on  │ │
│  └───────────────┘   └──────────────┘   │  4-8 GPUs)    │ │
│                                          └───────────────┘ │
│                                                             │
│  ┌───────────────┐   ┌──────────────┐                      │
│  │ KV Cache Mgr  │   │ Token        │                      │
│  │ (PagedAttn)   │   │ Streamer     │ ← SSE/WebSocket     │
│  │ 80GB managed  │   │ (yield tokens│    to client         │
│  └───────────────┘   │  as generated)│                      │
│                      └──────────────┘                      │
└────────────────────────────────────────────────────────────┘
```

## Cost Optimization

```
On-demand A100: ~$3/hour
Spot/preemptible: ~$1/hour (70% cheaper, but can be interrupted)

Strategy:
  Baseline traffic:  on-demand instances (always available)
  Spike traffic:     spot instances (cheap, tolerate interruption)
  Overnight/idle:    scale to zero (pay nothing)

Monthly cost for 1 LLM endpoint (70B model, 4×A100):
  24/7 on-demand:  $12/hour × 720 hours = $8,640/month
  With autoscaling: ~$3,000/month (scale down nights/weekends)
  With spot mix:    ~$1,500/month
```

## Interview Talking Points

> "The serving platform uses vLLM with continuous batching and PagedAttention for LLM workloads. Autoscaler watches request queue depth — when queue exceeds 10 pending requests, it launches a new replica. Scale-to-zero for idle models saves ~60% on GPU costs. For deployment, we use canary rollout: 5% traffic to new version, monitor P99 latency, then gradually increase to 100%."

> "For multi-model serving, small models share GPUs via MIG slicing — an A100 split into 7 instances, each running a different classifier. For the LLM, we need full GPUs with tensor parallelism across 4 A100s. Dynamic batching is critical: waiting 5ms to batch requests together gives 8x throughput improvement at the cost of 5ms added latency."
