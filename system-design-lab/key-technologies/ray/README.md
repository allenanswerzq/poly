# Ray — Distributed Computing Framework

## Overview

Ray is a **unified framework for scaling AI/ML workloads**. It handles distributed training, hyperparameter tuning, serving, and reinforcement learning on a cluster of machines. Think of it as "distributed Python made easy."

## History & Why It Exists

```
The problem (2017):
  ML workloads were getting complex:
    - Train a model on 8 GPUs (distributed training)
    - Run 100 hyperparameter trials in parallel
    - Serve the model with auto-scaling
    - RL agents need to simulate + learn simultaneously

  Each of these had a SEPARATE tool:
    Distributed training: Horovod
    Hyperparameter tuning: Optuna, Hyperopt
    Serving: TF Serving, TorchServe
    RL: custom code
    Data processing: Spark, Dask

  Robert Nishihara and Philipp Moritz at UC Berkeley (Ion Stoica's
  group — same lab that created Spark) asked: what if ONE framework
  handled all of these? Distribute ANY Python function across a cluster.

  Ray's insight: instead of specialized frameworks, provide primitives:
    @ray.remote turns any function into a distributed task.
    @ray.remote turns any class into a distributed actor.
    Ray handles scheduling, fault tolerance, data transfer.
    Build any distributed app on top of these primitives.

Timeline:
  2017  Ray paper published at OSDI (UC Berkeley RISELab)
  2019  Anyscale Inc. founded (commercial Ray support)
  2020  Ray 1.0 (stable API, Ray Serve, Ray Tune)
  2022  Ray 2.0 (Ray AIR — unified ML runtime)
  2023  Ray becomes the standard for LLM training and serving
        (used by OpenAI, Anthropic, many LLM companies)
  2024  Ray 2.9+ (compiled DAGs for low-latency serving)

Why Ray won the ML infrastructure market:
  1. SIMPLE API: @ray.remote and you're distributed. 5 lines of code.
  2. Python-native: not a JVM-based system like Spark.
  3. Unified: train + tune + serve in one framework.
  4. LLM era: Ray powers most LLM training pipelines.
  5. Ecosystem: Ray Train, Ray Tune, Ray Serve, Ray Data.

Who uses it:
  OpenAI, Anthropic, Uber, Spotify, Instacart, Netflix, ByteDance,
  Shopify. Most LLM training infrastructure runs on Ray.
```

## Why Ray?

```
Without Ray:
  You write: train_model()           ← runs on 1 machine
  To scale:  rewrite with MPI, gRPC, custom job scheduling... weeks of work

With Ray:
  You write: ray.remote(train_model) ← runs on 100 machines
  Ray handles: scheduling, fault tolerance, data transfer, GPU allocation
```

## Core Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Ray Cluster                                 │
│                                                                  │
│  ┌──────────────┐                                               │
│  │  Head Node    │                                               │
│  │  ┌──────────┐│  GCS (Global Control Store):                  │
│  │  │ GCS      ││  - Actor/task metadata                        │
│  │  │ Scheduler││  - Node status                                │
│  │  │ Dashboard ││  - Object locations                           │
│  │  └──────────┘│                                               │
│  └──────┬───────┘                                               │
│         │                                                        │
│  ┌──────▼───────┐  ┌──────────────┐  ┌──────────────┐         │
│  │ Worker Node 1│  │ Worker Node 2│  │ Worker Node 3│         │
│  │ 8×A100 GPUs  │  │ 8×A100 GPUs  │  │ 8×A100 GPUs  │         │
│  │ ┌──────────┐ │  │ ┌──────────┐ │  │ ┌──────────┐ │         │
│  │ │ Raylet   │ │  │ │ Raylet   │ │  │ │ Raylet   │ │         │
│  │ │ (local   │ │  │ │ (local   │ │  │ │ (local   │ │         │
│  │ │ scheduler│ │  │ │ scheduler│ │  │ │ scheduler│ │         │
│  │ │ + object │ │  │ │ + object │ │  │ │ + object │ │         │
│  │ │  store)  │ │  │ │  store)  │ │  │ │  store)  │ │         │
│  │ └──────────┘ │  │ └──────────┘ │  │ └──────────┘ │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
└─────────────────────────────────────────────────────────────────┘
```

## Key Concepts

### 1. Tasks — Stateless Remote Functions

```python
@ray.remote
def process_batch(data):
    return expensive_computation(data)

# Run 1000 tasks in parallel across the cluster
futures = [process_batch.remote(batch) for batch in batches]
results = ray.get(futures)  # blocks until all done
```

### 2. Actors — Stateful Remote Objects

```python
@ray.remote
class ModelServer:
    def __init__(self, model_path):
        self.model = load_model(model_path)

    def predict(self, input):
        return self.model(input)

# Create actor on a GPU node
server = ModelServer.remote("model.pt")
result = ray.get(server.predict.remote(data))
```

### 3. Ray Libraries

| Library | Purpose | Replaces |
|---------|---------|----------|
| **Ray Train** | Distributed training | Horovod, custom distributed code |
| **Ray Tune** | Hyperparameter tuning | Optuna (but distributed) |
| **Ray Serve** | Model serving | Flask + custom infra |
| **Ray Data** | Data processing pipelines | Spark (for ML data prep) |
| **Ray RLlib** | Reinforcement learning | Custom RL distributed code |

### 4. Object Store (Apache Arrow / Plasma)

```
Shared memory object store on each node:
  - Zero-copy reads between tasks on same node
  - Objects stored in shared memory (numpy arrays, tensors)
  - Automatic spill to disk if memory full

ray.put(large_array)  →  stored in shared memory
                          all tasks on this node read it without copying
```

### 5. Autoscaling

```
Ray Autoscaler:
  - Monitors pending task queue
  - Launches new nodes when queue grows
  - Terminates idle nodes after timeout
  - Works with AWS, GCP, Azure, K8s

Config:
  min_workers: 2
  max_workers: 100
  idle_timeout_minutes: 5
```

## When to Use Ray vs Alternatives

| Use Case | Ray | Alternative |
|----------|-----|-------------|
| Distributed training | Ray Train | PyTorch DDP, DeepSpeed |
| Hyperparameter tuning | Ray Tune | Optuna, Hyperopt |
| Model serving | Ray Serve | vLLM, Triton, TGI |
| Data processing | Ray Data | Spark, Dask |
| RL training | Ray RLlib | Custom |
| General distributed | Ray Core | Celery, Dask |

## Interview Sound Bite

> "I'd use Ray to orchestrate the distributed training pipeline. Ray Train handles data-parallel training across GPU nodes with automatic gradient synchronization. For hyperparameter tuning, Ray Tune runs hundreds of trials in parallel with early stopping (ASHA scheduler) to efficiently search the space. The Ray autoscaler provisions GPU nodes on demand and releases them when idle to control costs."
