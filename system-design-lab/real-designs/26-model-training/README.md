# Design a Model Training Platform (like SageMaker / Vertex AI)

## Problem Statement

Design a platform that allows ML engineers to:
- Submit training jobs with code + data + config
- Train on distributed GPU clusters
- Track experiments (metrics, hyperparameters, artifacts)
- Auto-scale GPU resources based on demand
- Save and version checkpoints

## Requirements

### Functional
- Submit training job (code, dataset, hyperparams, GPU requirements)
- Distributed training across multiple GPUs/nodes
- Real-time training metrics (loss, accuracy per step)
- Checkpoint saving + resume from failure
- Experiment tracking (compare runs)
- Model registry (save trained models with metadata)

### Non-Functional
- Scale: 1000s of concurrent training jobs
- GPU utilization: >80% (GPUs are $$$, don't waste them)
- Fault tolerance: resume from last checkpoint on failure
- Latency: job start <2 minutes (scheduling + data loading)

## High-Level Architecture

```
┌──────────────┐     ┌─────────────────┐     ┌─────────────────────────┐
│ ML Engineer  │────►│  API / Web UI    │────►│   Job Scheduler          │
│ (submit job) │     │  (job config)    │     │   (queue + prioritize)   │
└──────────────┘     └─────────────────┘     └────────────┬────────────┘
                                                           │
                                              ┌────────────▼────────────┐
                                              │   Resource Manager       │
                                              │   (allocate GPUs)        │
                                              └────────────┬────────────┘
                                                           │
                              ┌─────────────────┬──────────┴──────────┐
                              │                 │                     │
                         ┌────▼─────┐     ┌─────▼────┐     ┌────────▼────┐
                         │ Worker 0 │     │ Worker 1 │     │ Worker 2   │
                         │ (4×A100) │     │ (4×A100) │     │ (4×A100)   │
                         └────┬─────┘     └─────┬────┘     └────────┬───┘
                              │                 │                    │
                              │    AllReduce (gradient sync)         │
                              └─────────────────┴────────────────────┘
                                                │
                                    ┌───────────▼───────────┐
                                    │   Shared Storage       │
                                    │   (S3/GCS: data,       │
                                    │    checkpoints, models) │
                                    └───────────────────────┘
```

## Key Components

### 1. Job Scheduler
```
Job queue with priority:
  Priority 0: production retraining (highest)
  Priority 1: experiment runs
  Priority 2: hyperparameter sweeps
  Priority 3: ad-hoc exploration (lowest)

Bin-packing: fit jobs onto GPU nodes efficiently
  Job A needs 4 GPUs → assign to Node 1
  Job B needs 2 GPUs → pack onto Node 2 (still has 2 free)
  Job C needs 8 GPUs → needs 2 nodes, multi-node training

Preemption: low-priority jobs can be evicted for high-priority
  → Must checkpoint before eviction!
```

### 2. Distributed Training (Data Parallelism)
```
Single GPU:
  Model on 1 GPU, process all data sequentially

Data Parallel (most common):
  Same model on N GPUs, split data into N chunks
  Each GPU computes gradients on its chunk
  AllReduce: average gradients across all GPUs
  All GPUs update weights identically

  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐
  │  GPU 0  │  │  GPU 1  │  │  GPU 2  │  │  GPU 3  │
  │ Batch 0 │  │ Batch 1 │  │ Batch 2 │  │ Batch 3 │
  │ ∇Loss₀  │  │ ∇Loss₁  │  │ ∇Loss₂  │  │ ∇Loss₃  │
  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘
       └──────────┬─┴──────────┬─┘─────────────┘
              AllReduce (avg gradients)
                  │
          All GPUs get same averaged gradient
          All GPUs update weights identically

Scaling: 4 GPUs ≈ 3.5x speedup (communication overhead)
         64 GPUs ≈ 50x speedup (diminishing returns)
```

### 3. Checkpointing
```
Every N steps:
  Save model weights, optimizer state, step count to S3

On failure:
  Restart job → load last checkpoint → resume from step N

Checkpoint size: model_size × 3 (weights + optimizer × 2)
  GPT-3 (175B params): ~1TB per checkpoint
  → Save every 1000 steps, keep last 3 checkpoints
```

### 4. Experiment Tracking
```
Run 1: lr=0.001, batch=32  → loss=0.45, acc=0.91
Run 2: lr=0.003, batch=64  → loss=0.38, acc=0.93  ← best
Run 3: lr=0.010, batch=128 → loss=0.52, acc=0.88

Store per run: hyperparams, metrics (per step), code version, artifacts
Tools: MLflow, Weights & Biases, Neptune
```

### 5. Model Registry
```
model: "recommendation-v2"
  version 1: accuracy=0.91, deployed=false
  version 2: accuracy=0.93, deployed=true  ← production
  version 3: accuracy=0.94, deployed=false  (pending review)

Metadata: training job ID, dataset version, metrics, deploy status
```

## Database Schema

```sql
CREATE TABLE training_jobs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    status VARCHAR NOT NULL,  -- queued, running, completed, failed
    config JSONB NOT NULL,    -- hyperparams, GPU count, etc.
    gpu_count INTEGER,
    priority INTEGER DEFAULT 1,
    created_at TIMESTAMP,
    started_at TIMESTAMP,
    completed_at TIMESTAMP
);

CREATE TABLE metrics (
    job_id UUID REFERENCES training_jobs(id),
    step INTEGER,
    metric_name VARCHAR,
    metric_value FLOAT,
    timestamp TIMESTAMP,
    PRIMARY KEY (job_id, step, metric_name)
);

CREATE TABLE checkpoints (
    id UUID PRIMARY KEY,
    job_id UUID REFERENCES training_jobs(id),
    step INTEGER,
    storage_path TEXT,    -- s3://bucket/checkpoints/job-123/step-5000
    size_bytes BIGINT,
    created_at TIMESTAMP
);

CREATE TABLE models (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    version INTEGER NOT NULL,
    job_id UUID REFERENCES training_jobs(id),
    artifact_path TEXT,
    metrics JSONB,
    deployed BOOLEAN DEFAULT false,
    UNIQUE(name, version)
);
```

## Interview Talking Points

> "The scheduler uses bin-packing to maximize GPU utilization — GPUs cost $2/hour each, so leaving them idle is expensive. Jobs are prioritized: production retraining preempts experiment runs. Preempted jobs resume from their last checkpoint."

> "For distributed training, we use data parallelism with AllReduce for gradient synchronization. Each GPU processes a different mini-batch, then gradients are averaged across all GPUs. With 8 GPUs we get ~7x speedup due to communication overhead."

> "Checkpoints are saved to S3 every 1000 steps. On failure, the job restarts and loads the last checkpoint. For a 70B parameter model, each checkpoint is ~400GB, so we only keep the last 3."
