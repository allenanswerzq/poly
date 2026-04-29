# Ray — Distributed Computing Framework

---

## 1. What Ray Is and Why It Exists

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
    Data processing: Spark, Dask

  Robert Nishihara and Philipp Moritz at UC Berkeley (Ion Stoica's
  group — same lab that created Spark) asked: what if ONE framework
  handled all of these?

  Ray's insight: instead of specialized frameworks, provide two primitives:
    @ray.remote on a function → distributed task (stateless)
    @ray.remote on a class   → distributed actor (stateful)
    Everything else is built on top of these.

Timeline:
  2017  Ray paper at OSDI (UC Berkeley RISELab)
  2019  Anyscale Inc. founded (commercial Ray support)
  2020  Ray 1.0 (stable API)
  2023  Ray becomes the standard for LLM training/serving
        (used by OpenAI, Anthropic, most LLM companies)

Who uses it:
  OpenAI, Anthropic, Uber, Spotify, Netflix, ByteDance, Shopify.
```

---

## 2. The Two Primitives

```python
# Primitive 1: TASK — stateless remote function
@ray.remote
def process(data):
    return expensive_computation(data)

futures = [process.remote(batch) for batch in batches]  # launch 1000 tasks
results = ray.get(futures)                               # block until done

# Primitive 2: ACTOR — stateful remote object
@ray.remote
class Counter:
    def __init__(self):
        self.n = 0
    def increment(self):
        self.n += 1
        return self.n

counter = Counter.remote()                     # lives on a remote node
ray.get(counter.increment.remote())            # → 1
ray.get(counter.increment.remote())            # → 2 (state persists)
```

```
The difference:

  TASK:
    - Stateless. Each call is independent.
    - Can run anywhere. Can retry on failure (idempotent).
    - Good for: data processing, batch inference, map-reduce.

  ACTOR:
    - Stateful. Has a long-lived object on a specific node.
    - Method calls are serialized (one at a time, in order).
    - Can't just retry — state might be inconsistent.
    - Good for: model serving, parameter servers, simulators.
```

---

## 3. Cluster Architecture — What Runs Where

```
┌─────────────────────────────────────────────────────────────────────┐
│                         RAY CLUSTER                                  │
│                                                                      │
│  HEAD NODE                                                          │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                                                                │ │
│  │  GCS (Global Control Store)                                   │ │
│  │  ┌──────────────────────────────────────────────────────────┐ │ │
│  │  │  The "brain" of the cluster. A key-value store that      │ │ │
│  │  │  tracks ALL metadata:                                    │ │ │
│  │  │    - Which nodes are alive                               │ │ │
│  │  │    - Where each actor lives (node + PID)                 │ │ │
│  │  │    - Object locations (which node has which object)      │ │ │
│  │  │    - Resource availability (GPUs, CPUs per node)         │ │ │
│  │  │    - Task lineage (for reconstruction on failure)        │ │ │
│  │  │                                                          │ │ │
│  │  │  Backed by Redis (Ray 1.x) or internal store (Ray 2.x)  │ │ │
│  │  │  Single point of truth. If GCS dies → cluster is down.   │ │ │
│  │  │  (Ray 2.x adds GCS fault tolerance via checkpointing.)  │ │ │
│  │  └──────────────────────────────────────────────────────────┘ │ │
│  │                                                                │ │
│  │  Global Scheduler (only for first hop — see section 4)        │ │
│  │  Autoscaler (monitors load, adds/removes nodes)               │ │
│  │  Dashboard (web UI for monitoring)                             │ │
│  │                                                                │ │
│  │  Also runs a Raylet (same as worker nodes)                    │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  WORKER NODE 1                 WORKER NODE 2                        │
│  ┌────────────────────────┐   ┌────────────────────────┐           │
│  │                        │   │                        │           │
│  │  Raylet (C++ process)  │   │  Raylet (C++ process)  │           │
│  │  ┌──────────────────┐  │   │  ┌──────────────────┐  │           │
│  │  │                  │  │   │  │                  │  │           │
│  │  │ Local Scheduler  │  │   │  │ Local Scheduler  │  │           │
│  │  │ - picks which    │  │   │  │                  │  │           │
│  │  │   worker runs    │  │   │  │                  │  │           │
│  │  │   each task      │  │   │  │                  │  │           │
│  │  │                  │  │   │  │                  │  │           │
│  │  │ Object Store     │  │   │  │ Object Store     │  │           │
│  │  │ (shared memory)  │  │   │  │ (shared memory)  │  │           │
│  │  │ - Apache Arrow   │  │   │  │                  │  │           │
│  │  │ - zero-copy      │  │   │  │                  │  │           │
│  │  │   reads          │  │   │  │                  │  │           │
│  │  └──────────────────┘  │   │  └──────────────────┘  │           │
│  │                        │   │                        │           │
│  │  Worker processes:     │   │  Worker processes:     │           │
│  │  ┌─────┐ ┌─────┐      │   │  ┌─────┐ ┌─────┐      │           │
│  │  │ W1  │ │ W2  │ ...  │   │  │ W1  │ │ W2  │ ...  │           │
│  │  │(py) │ │(py) │      │   │  │(py) │ │(py) │      │           │
│  │  └─────┘ └─────┘      │   │  └─────┘ └─────┘      │           │
│  │  8×A100 GPUs           │   │  8×A100 GPUs           │           │
│  └────────────────────────┘   └────────────────────────┘           │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘

Per-node process map:
  Raylet (1 per node):
    - C++ process. Runs the local scheduler + object store.
    - Manages all worker processes on this node.
    - Communicates with GCS and other Raylets.

  Worker process (many per node):
    - Python process. Runs actual user tasks/actors.
    - One worker per task/actor at a time.
    - Workers are REUSED — after a task finishes, the worker
      goes back to the pool for the next task.
```

---

## 4. How Task Scheduling Actually Works

```
You call: process.remote(data)

What happens under the hood:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │ Step 1: DRIVER serializes the task                              │
  │   Your Python process (the "driver") creates a task spec:       │
  │     { function_id: hash(process),                               │
  │       args: [ObjectRef(data)],                                  │
  │       resources: {CPU: 1},          ← from @ray.remote(num_cpus=1)
  │       task_id: unique_id }                                      │
  │                                                                  │
  │ Step 2: DRIVER submits to LOCAL Raylet                          │
  │   The driver talks to the Raylet on its own node via shared     │
  │   memory (not RPC — fast).                                      │
  │                                                                  │
  │ Step 3: LOCAL Raylet tries to schedule LOCALLY first            │
  │   "Do I have a free CPU on this node?"                          │
  │   YES → assign to a local idle worker process. Done.            │
  │   NO  → go to step 4.                                          │
  │                                                                  │
  │ Step 4: LOCAL Raylet asks GCS for another node                  │
  │   GCS knows resource availability across all nodes.             │
  │   GCS picks a node with available resources.                    │
  │   The task spec is sent to that node's Raylet.                  │
  │                                                                  │
  │ Step 5: REMOTE Raylet receives task                             │
  │   It assigns an idle worker process on its node.                │
  │   If the worker doesn't have the function loaded, it imports it.│
  │                                                                  │
  │ Step 6: WORKER executes the function                            │
  │   - Deserializes arguments                                      │
  │   - If args are ObjectRefs on another node → fetch from         │
  │     that node's object store first (see section 5)              │
  │   - Runs the function                                           │
  │   - Serializes the result → puts it in local object store       │
  │                                                                  │
  │ Step 7: DRIVER calls ray.get(future)                            │
  │   - Blocks until the result is available                        │
  │   - If result is on a remote node, fetches it to local store    │
  │   - Deserializes and returns to Python                          │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

KEY DESIGN: scheduling is BOTTOM-UP, not top-down.
  Each Raylet tries to schedule locally first.
  Only escalates to GCS if it can't.
  This avoids the GCS becoming a bottleneck.

  Old Ray (v0.x) had a centralized global scheduler.
  It couldn't scale past ~100 nodes.
  The distributed scheduler (Ray 1.0+) scales to 1000+ nodes.
```

---

## 5. Object Store — How Data Moves

```
Every node has an OBJECT STORE (in shared memory, backed by Apache Arrow).

This is the secret to Ray's performance for ML workloads:
  Large tensors (GBs) can be shared between tasks WITHOUT copying.

How it works:

  ┌────────────────────────────────────────────────────────────────┐
  │ NODE 1                                                        │
  │                                                                │
  │  Driver:                                                      │
  │    ref = ray.put(huge_numpy_array)                            │
  │    # Serializes array into shared memory (mmap'd)             │
  │    # Returns an ObjectRef (a pointer, not the data)           │
  │                                                                │
  │  Object Store (shared memory, e.g. /dev/shm):                 │
  │    ┌─────────────────────────────────────────┐                │
  │    │  ObjectID_abc → [100 MB numpy array]    │                │
  │    │  ObjectID_def → [50 MB tensor]          │                │
  │    │  ObjectID_ghi → [small Python dict]     │                │
  │    └─────────────────────────────────────────┘                │
  │                                                                │
  │  Worker 1 reads ObjectID_abc:                                 │
  │    → ZERO COPY. Worker's numpy array points directly          │
  │      into the shared memory region. No memcpy.                │
  │      (Apache Arrow format makes this possible for             │
  │       numpy arrays, pandas DataFrames, etc.)                  │
  │                                                                │
  │  Worker 2 also reads ObjectID_abc:                            │
  │    → Also zero copy. Same shared memory page.                 │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘

  What if a task on NODE 2 needs an object from NODE 1?

  ┌───────────────┐                    ┌───────────────┐
  │    NODE 1     │                    │    NODE 2     │
  │               │                    │               │
  │ Object Store: │   gRPC transfer   │ Object Store: │
  │ [ObjID_abc]   │ ◄──────────────── │ "I need abc"  │
  │               │ ──────────────────►│               │
  │               │   sends the bytes  │ [ObjID_abc]   │
  │               │                    │ (now local)   │
  └───────────────┘                    └───────────────┘

  The transfer happens AUTOMATICALLY when a task tries to
  access an ObjectRef that's on another node.

  Flow:
    1. Worker on Node 2 calls ray.get(ref) or uses ref as arg
    2. Local Raylet checks: "do I have this object?" → NO
    3. Raylet asks GCS: "where is ObjectID_abc?"
    4. GCS responds: "Node 1 has it"
    5. Raylet pulls the object from Node 1's object store
    6. Object is now in Node 2's local object store
    7. Worker gets zero-copy access to it

  Memory management:
    - Objects are reference-counted
    - When no tasks reference an object → eligible for eviction
    - If object store is full → LRU eviction to disk (spilling)
    - Spilled objects are re-fetched from disk when needed
```

---

## 6. How Actors Work Under The Hood

```
@ray.remote
class ModelServer:
    def __init__(self, path):
        self.model = load(path)
    def predict(self, x):
        return self.model(x)

server = ModelServer.remote("model.pt")

What happens:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │ Step 1: Actor creation                                          │
  │   Driver sends to GCS: "create actor ModelServer with these     │
  │   resource requirements (e.g., num_gpus=1)"                     │
  │                                                                  │
  │ Step 2: GCS finds a node with a free GPU                       │
  │   Picks Node 2. Tells Node 2's Raylet: "start this actor."     │
  │                                                                  │
  │ Step 3: Raylet on Node 2 starts a DEDICATED worker process     │
  │   Unlike tasks (which reuse workers from a pool),               │
  │   actors get their OWN process that lives as long as the actor. │
  │   The __init__ method runs. Model is loaded into GPU memory.    │
  │                                                                  │
  │ Step 4: GCS records actor location                              │
  │   { actor_id: "abc", node: "node-2", pid: 12345,               │
  │     resources: {GPU: 1} }                                       │
  │                                                                  │
  │ Step 5: Method calls                                            │
  │   server.predict.remote(data) creates a task:                   │
  │     { type: ACTOR_TASK, actor_id: "abc", method: "predict",     │
  │       args: [ObjectRef(data)] }                                 │
  │                                                                  │
  │   This task is sent DIRECTLY to Node 2's Raylet                 │
  │   (because GCS already told us where the actor lives).          │
  │   No scheduling needed — it goes to that specific worker.       │
  │                                                                  │
  │ Step 6: Method calls are QUEUED and executed ONE AT A TIME      │
  │   Actor methods are serialized. No concurrent execution.        │
  │   This guarantees state consistency — no locks needed.          │
  │                                                                  │
  │   client1: server.predict.remote(x1)  ─┐                       │
  │   client2: server.predict.remote(x2)   ├─ queued in order       │
  │   client3: server.predict.remote(x3)  ─┘   → executed serially │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

Actor lifecycle:
  Created  → runs __init__
  Running  → accepts method calls (queued, serial execution)
  Failed   → can be restarted (max_restarts=N, max_task_retries=M)
  Deleted  → when handle goes out of scope / explicitly killed
```

---

## 7. Fault Tolerance

```
Two kinds of failures, handled differently:

TASK FAILURE:
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Task is stateless → just re-execute it.                    │
  │                                                              │
  │  1. Worker process crashes while running task                │
  │  2. Raylet detects worker death (heartbeat timeout)          │
  │  3. Raylet marks task as failed                              │
  │  4. Task is resubmitted to another worker                   │
  │     (up to max_retries, default 3)                          │
  │                                                              │
  │  What about the task's INPUT data?                          │
  │    If the input object was lost (node died):                │
  │    → Ray can RECONSTRUCT it by re-executing the task that   │
  │      created it (lineage reconstruction).                   │
  │    → GCS stores task lineage: "ObjectID_abc was created by  │
  │      task_xyz with args [...]"                              │
  │    → Re-run task_xyz to recreate ObjectID_abc.              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

ACTOR FAILURE:
  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  Actor has STATE → can't just re-execute.                   │
  │                                                              │
  │  Option 1: max_restarts=0 (default)                         │
  │    Actor dies → all pending method calls raise exception.    │
  │    Application must handle it.                              │
  │                                                              │
  │  Option 2: max_restarts=N                                   │
  │    Actor dies → Ray restarts it on a new node.              │
  │    __init__ runs again. State is LOST.                      │
  │    Pending/future method calls are retried on new actor.    │
  │    Application must make __init__ reconstruct state         │
  │    (e.g., reload model from disk — this usually works for   │
  │    ML serving since model weights are loaded from file).    │
  │                                                              │
  │  Option 3: Checkpointing (application-level)                │
  │    Actor periodically saves state to external storage.      │
  │    On restart, __init__ restores from checkpoint.           │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

NODE FAILURE:
  1. GCS detects node death (heartbeat stops)
  2. All actors on that node are restarted (if max_restarts > 0)
  3. All objects on that node are lost — reconstructed via lineage
  4. All tasks on that node are retried on other nodes
```

---

## 8. How Ray Train Works (Distributed Training)

```
The most common Ray use case: distributed GPU training.

Without Ray:
  torchrun --nproc_per_node=8 --nnodes=4 train.py
  → manual node setup, SSH keys, shared filesystem, etc.

With Ray:
  Ray Train launches training workers as Ray actors.

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  trainer = TorchTrainer(                                        │
  │      train_func,                                                │
  │      scaling_config=ScalingConfig(                               │
  │          num_workers=32,         # 32 training workers          │
  │          use_gpu=True,           # each gets 1 GPU             │
  │          resources_per_worker={"GPU": 1, "CPU": 4}             │
  │      )                                                          │
  │  )                                                              │
  │  result = trainer.fit()                                         │
  │                                                                  │
  │  What happens:                                                  │
  │                                                                  │
  │  1. Ray creates 32 ACTOR workers across the cluster             │
  │     Each actor gets 1 GPU (Ray's resource system ensures this)  │
  │                                                                  │
  │  2. Each actor runs train_func() in its own process             │
  │     Inside train_func, PyTorch DDP is set up automatically:     │
  │       - NCCL backend for GPU communication                      │
  │       - Ray provides the rendezvous info (master addr/port)     │
  │       - Each worker gets its rank and world_size                │
  │                                                                  │
  │  3. Training loop runs with standard PyTorch DDP                │
  │     Each worker:                                                │
  │       - Gets a shard of the data (DataLoader with DistributedSampler)
  │       - Forward pass on its shard                               │
  │       - Backward pass (compute local gradients)                 │
  │       - AllReduce (NCCL) to average gradients across all workers│
  │       - Optimizer step (all workers now have identical weights)  │
  │                                                                  │
  │  4. Ray handles:                                                │
  │     - Node provisioning (autoscaler)                            │
  │     - Worker failure → restart training from last checkpoint    │
  │     - Metric reporting (loss, accuracy → Ray dashboard)         │
  │     - Checkpoint saving to shared storage                       │
  │                                                                  │
  │  Ray's value: the INFRASTRUCTURE around training.               │
  │  The actual gradient math is still PyTorch / NCCL.              │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘

  Node 1                    Node 2                    Node 3
  ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
  │ Worker 0 (GPU 0) │     │ Worker 8 (GPU 0) │     │ Worker 16(GPU 0) │
  │ Worker 1 (GPU 1) │     │ Worker 9 (GPU 1) │     │ Worker 17(GPU 1) │
  │ Worker 2 (GPU 2) │     │ Worker 10(GPU 2) │     │ Worker 18(GPU 2) │
  │ ...               │     │ ...               │     │ ...               │
  │ Worker 7 (GPU 7) │     │ Worker 15(GPU 7) │     │ Worker 23(GPU 7) │
  └──────────────────┘     └──────────────────┘     └──────────────────┘
           │                        │                        │
           └────────── NCCL AllReduce (gradients) ──────────┘
                    (direct GPU-to-GPU, via NVLink + InfiniBand)
```

---

## 9. Resource Management — How GPUs Get Assigned

```
Every node reports its resources to GCS:
  Node 1: {CPU: 64, GPU: 8, memory: 512GB}
  Node 2: {CPU: 64, GPU: 8, memory: 512GB}

Every task/actor declares what it needs:
  @ray.remote(num_gpus=1, num_cpus=4)
  def train_step(data): ...

  @ray.remote(num_gpus=0.5)   ← fractional GPUs! Two actors share 1 GPU
  class SmallModel: ...

  @ray.remote(resources={"TPU": 1})  ← custom resources
  def tpu_task(): ...

Scheduling:
  Task needs {GPU: 1, CPU: 4}
  Raylet checks: Node 1 has {GPU: 3 free, CPU: 20 free} → fits.
  Assigns the task. Decrements: {GPU: 2 free, CPU: 16 free}.

  IMPORTANT: Ray does NOT enforce GPU isolation.
  "num_gpus=1" means Ray sets CUDA_VISIBLE_DEVICES=<assigned_gpu>.
  The worker process only SEES that one GPU.
  But there's no hardware-level isolation (unlike VMs).
  Two workers on the same GPU (fractional) must cooperate.

Placement groups (for gang scheduling):
  Training needs all 32 workers to start TOGETHER.
  If only 20 GPUs are free → don't start 20 and wait for 12.
  Use a placement group: "all or nothing."

  pg = placement_group([{"GPU": 1}] * 32, strategy="STRICT_SPREAD")
  # STRICT_SPREAD: one bundle per node (spread across nodes)
  # STRICT_PACK: pack all bundles on fewest nodes
  # PACK: best-effort pack
```

---

## 10. Serialization — How Python Objects Cross Nodes

```
When you pass data between tasks/actors, Ray must serialize it.

  ray.put(my_object)        → serialize → bytes → object store
  ray.get(object_ref)       → fetch bytes → deserialize → Python object

Serialization stack:
  1. Apache Arrow (for numpy arrays, pandas DataFrames)
     → Zero-copy deserialization. No memcpy. Very fast.
     → This is why Ray is fast for ML — tensors stay in Arrow format.

  2. pickle (for everything else: Python dicts, custom objects)
     → Standard Python serialization. Slower. Involves memcpy.

  3. Custom serializers (you can register your own)

The zero-copy path for numpy:
  ray.put(np.zeros(100_000_000))
    → Arrow serializes: writes numpy data directly into shared memory
    → Other workers on the same node: their np.array is a VIEW
      into that same shared memory region. No copy at all.

  This is critical for ML:
    A 10 GB dataset loaded once → shared by all 8 workers on the node.
    Without zero-copy: 8 copies = 80 GB. With: 10 GB.
```

---

## 11. Communication Between Raylets

```
How nodes talk to each other:

  ┌───────────┐         gRPC          ┌───────────┐
  │  Raylet 1 │ ◄──────────────────── │  Raylet 2 │
  │           │ ──────────────────── ►│           │
  └───────────┘                        └───────────┘
        │                                    │
        │     gRPC                           │    gRPC
        ▼                                    ▼
  ┌───────────┐                        ┌───────────┐
  │    GCS    │                        │    GCS    │
  └───────────┘                        └───────────┘
        (same GCS — on head node)

  Raylet ↔ Raylet: gRPC for object transfers, task forwarding
  Raylet ↔ GCS:    gRPC for metadata (actor locations, resource updates)
  Raylet ↔ Worker: shared memory + Unix domain sockets (same machine, fast)
  Worker ↔ Worker (across nodes): goes through Raylets

  For ML training, the heavy data path (gradient sync) does NOT go
  through Ray at all — it uses NCCL directly (GPU-to-GPU over NVLink
  or InfiniBand). Ray just sets up the workers; NCCL does the comms.
```

---

## 12. Autoscaling

```
Ray Autoscaler runs on the head node. Watches resource demand vs supply.

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                  │
  │  Loop (every few seconds):                                      │
  │                                                                  │
  │    1. Check pending tasks/actors that can't be scheduled         │
  │       "50 tasks need GPU but no GPUs available"                 │
  │                                                                  │
  │    2. Calculate how many new nodes are needed                   │
  │       50 GPU tasks ÷ 8 GPUs per node = 7 new nodes             │
  │       (but max_workers caps it)                                 │
  │                                                                  │
  │    3. Request nodes from cloud provider                         │
  │       AWS: launch EC2 instances                                 │
  │       GCP: launch GCE VMs                                       │
  │       K8s: create pods                                          │
  │                                                                  │
  │    4. New nodes join cluster, register with GCS                 │
  │       Raylet starts, reports resources, ready for work.         │
  │                                                                  │
  │    5. Check for idle nodes (no tasks for idle_timeout_minutes)  │
  │       Idle → terminate node → save cost.                       │
  │                                                                  │
  └──────────────────────────────────────────────────────────────────┘
```

---

## 13. Ray vs. Alternatives

```
┌─────────────────┬──────────────────────────────────────────────────┐
│                 │ What it does differently from Ray                │
├─────────────────┼──────────────────────────────────────────────────┤
│ Spark           │ JVM-based. Map-reduce model. Great for ETL/SQL. │
│                 │ Bad for GPU, ML, Python-native workloads.       │
│                 │ Ray: Python-native, GPU-first, actor model.     │
├─────────────────┼──────────────────────────────────────────────────┤
│ Dask            │ Python-native like Ray. Good for pandas/numpy   │
│                 │ at scale. No actor model. Weaker GPU support.   │
│                 │ Ray: broader (training + serving + tuning).     │
├─────────────────┼──────────────────────────────────────────────────┤
│ Celery          │ Python task queue. Great for web backends.      │
│                 │ No shared memory, no GPU support, no actors.    │
│                 │ Ray: much faster for compute-heavy workloads.   │
├─────────────────┼──────────────────────────────────────────────────┤
│ Kubernetes      │ Container orchestration. Lower level. You still │
│                 │ need to write the distributed logic yourself.   │
│                 │ Ray runs ON K8s (KubeRay), handles the app layer│
├─────────────────┼──────────────────────────────────────────────────┤
│ DeepSpeed /     │ Specialized for distributed training only.      │
│ FSDP            │ More optimized for training (ZeRO, offloading). │
│                 │ Ray Train wraps these — uses them internally.   │
├─────────────────┼──────────────────────────────────────────────────┤
│ vLLM / TGI      │ Specialized for LLM inference only.            │
│                 │ Often run ON Ray (vLLM uses Ray for multi-GPU). │
└─────────────────┴──────────────────────────────────────────────────┘

Ray's niche: the GLUE between all these tools.
  Not the fastest at any single thing, but the only one that
  handles training + tuning + serving + data in one framework.
```

---

## 14. Key Numbers

```
Cluster scale:           1000+ nodes in production (Anyscale)
Task overhead:           ~1ms per task (scheduling + dispatch)
Object store throughput: ~10 GB/s per node (shared memory)
Cross-node transfer:     limited by network (~25-100 Gbps)
Actor method call:       ~0.5ms overhead
Max object size:         limited by node memory (can spill to disk)
Autoscale latency:       ~1-5 min (cloud VM boot time dominates)
GCS checkpoint:          every few seconds (Ray 2.x fault tolerance)
```
