# Slurm — HPC Job Scheduler

---

## 1. What Slurm Is

```
Slurm (Simple Linux Utility for Resource Management) is the
scheduler that runs on most HPC clusters and GPU training clusters.

  You have 2000 nodes with 16,000 GPUs.
  50 researchers all want to train models.
  Who gets which GPUs? For how long? What happens when a job crashes?

  Slurm answers all of this:
    - Job queuing (fair scheduling across users)
    - Resource allocation (GPUs, CPUs, memory per job)
    - Job launching (start processes on allocated nodes)
    - Monitoring (job status, node health)
    - Failure handling (detect bad nodes, restart jobs)

  Used by: Meta (LLaMA), NVIDIA, most universities, national labs,
  supercomputer centers (ORNL, NERSC, etc.)
```

---

## 2. Architecture

```
  ┌──────────────────────────────────────────────────────────┐
  │  SLURM CONTROLLER (slurmctld) — runs on master node     │
  │  ┌────────────────────────────────────────────────────┐  │
  │  │                                                    │  │
  │  │  Job Queue:    [job 001: 256 GPUs, user=alice]    │  │
  │  │                [job 002: 64 GPUs, user=bob]       │  │
  │  │                [job 003: 1024 GPUs, user=charlie] │  │
  │  │                                                    │  │
  │  │  Node State:   node001: 8 GPU allocated (job 001) │  │
  │  │                node002: idle                       │  │
  │  │                node003: 8 GPU allocated (job 001) │  │
  │  │                node004: drained (bad GPU)         │  │
  │  │                ...                                 │  │
  │  │                                                    │  │
  │  │  Scheduler:    backfill, fair-share, priority      │  │
  │  │  Accounting:   per-user GPU-hours tracking         │  │
  │  │                                                    │  │
  │  └────────────────────────────────────────────────────┘  │
  └────────────────────────────────────┬─────────────────────┘
                                       │
            ┌──────────────────────────┼────────────────────────┐
            │                          │                        │
            ▼                          ▼                        ▼
  ┌─────────────────┐       ┌─────────────────┐      ┌─────────────────┐
  │ COMPUTE NODE 1  │       │ COMPUTE NODE 2  │      │ COMPUTE NODE N  │
  │                 │       │                 │      │                 │
  │ slurmd daemon   │       │ slurmd daemon   │      │ slurmd daemon   │
  │ (agent on node) │       │                 │      │                 │
  │                 │       │                 │      │                 │
  │ 8× H100 GPUs   │       │ 8× H100 GPUs   │      │ 8× H100 GPUs   │
  │ 64 CPU cores    │       │ 64 CPU cores    │      │ 64 CPU cores    │
  │ 2 TB RAM        │       │ 2 TB RAM        │      │ 2 TB RAM        │
  └─────────────────┘       └─────────────────┘      └─────────────────┘

  slurmctld:  one process, on the master node. Makes all decisions.
  slurmd:     one per compute node. Executes jobs, reports status.
  slurmdbd:   (optional) database daemon for accounting/history.
```

---

## 3. How a Training Job Runs

```
You're a researcher. You want to train LLaMA-70B on 256 GPUs.

Step 1: Write a job script (train_70b.sh):

  #!/bin/bash
  #SBATCH --job-name=llama-70b
  #SBATCH --nodes=32                  # 32 nodes × 8 GPUs = 256 GPUs
  #SBATCH --ntasks-per-node=8        # 1 process per GPU
  #SBATCH --gpus-per-node=8          # use all 8 GPUs on each node
  #SBATCH --cpus-per-task=8          # 8 CPU threads per GPU process
  #SBATCH --mem=0                     # use all available memory
  #SBATCH --time=72:00:00            # max wall time: 3 days
  #SBATCH --partition=gpu            # use the GPU partition
  #SBATCH --exclusive                 # no other jobs on these nodes

  # Set up environment
  module load cuda/12.2 pytorch/2.3
  export MASTER_ADDR=$(scontrol show hostname $SLURM_NODELIST | head -1)
  export MASTER_PORT=29500
  export WORLD_SIZE=$SLURM_NTASKS

  # Launch training on all nodes
  srun python train.py \
      --model-size=70b \
      --tp=8 --pp=4 --dp=8 \
      --checkpoint-dir=/lustre/checkpoints/llama-70b


Step 2: Submit the job:

  $ sbatch train_70b.sh
  Submitted batch job 12345

Step 3: Slurm schedules it:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  slurmctld receives job 12345: needs 32 nodes, 256 GPUs.   │
  │                                                              │
  │  Scheduler checks:                                          │
  │    - Are 32 nodes available right now?                      │
  │      YES → allocate immediately.                            │
  │      NO  → put in queue. Wait for running jobs to finish.  │
  │                                                              │
  │  If queued, priority based on:                              │
  │    - Fair share: user's recent GPU-hour usage               │
  │      (used a lot recently? → lower priority)                │
  │    - Job size: larger jobs sometimes get priority           │
  │      (backfill: small jobs fill gaps while big jobs wait)   │
  │    - Partition priority: some queues have higher priority   │
  │    - Time in queue: longer wait → gradually higher priority │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

Step 4: Job starts:

  slurmctld tells slurmd on 32 nodes: "start job 12345."

  srun launches 256 processes (8 per node):
    Node 0: python train.py (rank 0-7)
    Node 1: python train.py (rank 8-15)
    ...
    Node 31: python train.py (rank 248-255)

  Each process gets:
    SLURM_PROCID:      global rank (0-255)
    SLURM_LOCALID:     local GPU ID (0-7)
    SLURM_NODELIST:    list of all allocated nodes
    CUDA_VISIBLE_DEVICES: assigned GPU on this node

Step 5: Monitor:

  $ squeue -u alice
  JOBID  PARTITION  NAME       USER   ST  TIME   NODES
  12345  gpu        llama-70b  alice  R   4:32   32

  $ sacct -j 12345 --format=JobID,Elapsed,MaxRSS,MaxVMSize,State
```

---

## 4. srun vs sbatch vs salloc

```
Three ways to interact with Slurm:

  sbatch:  Submit a BATCH JOB (most common for training).
           Job runs unattended. Output goes to a file.
           $ sbatch train.sh
           → Queued, runs when resources available.

  srun:    Run a command DIRECTLY on allocated nodes.
           Usually INSIDE an sbatch script.
           $ srun python train.py
           → Launches one copy per task across all nodes.
           Can also be used interactively with salloc.

  salloc:  Get an INTERACTIVE allocation.
           You get a shell on allocated nodes.
           $ salloc --nodes=4 --gpus-per-node=8
           → Now you have 32 GPUs. Run commands manually.
           → Useful for debugging, interactive development.
           → Expensive: GPUs idle while you think.
```

---

## 5. Partitions and QoS

```
Clusters are divided into PARTITIONS (like queues):

  ┌────────────────┬──────────┬───────────┬────────────────────────┐
  │ Partition      │ Nodes    │ Max time  │ Purpose                │
  ├────────────────┼──────────┼───────────┼────────────────────────┤
  │ gpu            │ 200      │ 72 hours  │ Normal training jobs   │
  │ gpu-large      │ 500      │ 168 hours │ Large-scale training   │
  │ dev            │ 10       │ 4 hours   │ Development, debugging │
  │ urgent         │ 50       │ 24 hours  │ Priority jobs (preempt)│
  └────────────────┴──────────┴───────────┴────────────────────────┘

QoS (Quality of Service) controls:
  - Max jobs per user
  - Max GPUs per user
  - Priority levels
  - Preemption (urgent jobs can kill lower-priority jobs)

  #SBATCH --partition=gpu-large
  #SBATCH --qos=high-priority
```

---

## 6. Failure Detection and Recovery

```
At 2000 nodes, hardware fails regularly. Slurm handles this:

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  NODE HEALTH CHECKING:                                      │
  │                                                              │
  │  slurmd sends heartbeats to slurmctld every 30 seconds.    │
  │  If 3 heartbeats missed → node marked DOWN.                │
  │                                                              │
  │  Prolog/Epilog scripts (run before/after every job):        │
  │    Prolog: check GPUs (nvidia-smi), check IB (ibstat),     │
  │            check filesystem mount (/lustre), check RAM.     │
  │    If any check fails → node marked DRAINED.               │
  │    Drained nodes don't receive new jobs.                    │
  │                                                              │
  │  JOB FAILURE HANDLING:                                      │
  │                                                              │
  │  If any task in a job fails:                                │
  │    - Default: entire job is killed.                         │
  │    - All 256 processes killed on all 32 nodes.              │
  │                                                              │
  │  Auto-restart (if configured):                              │
  │    #SBATCH --requeue                                        │
  │    Job goes back to the front of the queue.                 │
  │    Starts fresh on new set of healthy nodes.                │
  │    Training code reads last checkpoint → resumes.           │
  │                                                              │
  │  External watchdog (what Meta actually does):               │
  │    A monitoring script watches each training job:           │
  │    - If loss is NaN → kill and restart                     │
  │    - If throughput drops >20% → suspect bad GPU, restart    │
  │    - If NCCL hangs >30 min → kill and restart              │
  │    - If node unresponsive → drain node, restart job         │
  │    - Automatically exclude bad nodes from next run          │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘

Typical restart cycle for a large training run:

  1. Job 12345 running on 32 nodes.
  2. GPU on node017 has ECC error.
  3. NCCL operation on that GPU hangs.
  4. After 5 min timeout, rank 136 (on node017) crashes.
  5. srun detects task failure → kills all 256 tasks.
  6. Slurm marks node017 as DRAINED.
  7. Job is requeued (--requeue flag).
  8. Slurm allocates 32 DIFFERENT nodes (excluding node017).
  9. Job restarts. train.py loads last checkpoint.
  10. Resumes training. Lost ~10-30 minutes of work.
```

---

## 7. Fair Share Scheduling

```
Multiple users compete for GPUs. Slurm's fair-share algorithm:

  Each user has a "share" (configured by admin):
    alice: 30%    (training large models, higher allocation)
    bob:   20%
    charlie: 20%
    others: 30%

  If alice used 50% of GPU-hours last week:
    Her effective priority DROPS (over-used her share).
    bob and charlie's priority RISES (under-used).

  This naturally balances without strict quotas:
    - Alice can burst to 100% when nobody else needs GPUs.
    - But when bob submits a job, he gets priority because
      alice has been over-consuming.

  Backfill scheduler:
    Big job (1024 GPUs) is waiting for resources.
    A small job (8 GPUs) can fit in the gap and finish
    before those 1024 GPUs become available.
    → Run the small job NOW, it doesn't delay the big job.
    → Keeps utilization high.
```

---

## 8. Slurm vs Kubernetes for ML Training

```
┌──────────────────┬──────────────────┬───────────────────────────┐
│                  │ Slurm            │ Kubernetes                │
├──────────────────┼──────────────────┼───────────────────────────┤
│ Origin           │ HPC (1990s era)  │ Cloud-native (2014)       │
│ Scheduling unit  │ Process          │ Container (pod)           │
│ GPU support      │ Native, mature   │ Device plugin (add-on)    │
│ Network          │ InfiniBand aware │ Ethernet-focused          │
│ Multi-tenancy    │ Users, fair-share│ Namespaces, quotas        │
│ Failure handling │ Requeue jobs     │ Restart pods              │
│ Ecosystem        │ MPI, OpenMP, NCCL│ Docker, Helm, microservices│
│ Learning curve   │ Simple (sbatch)  │ Complex (YAML, operators) │
│ Scale tested     │ 100K+ nodes      │ ~15K nodes                │
│ Used by          │ Meta, NVIDIA,    │ OpenAI (Azure), startups  │
│                  │ universities     │                           │
├──────────────────┼──────────────────┼───────────────────────────┤
│ Best for         │ Dedicated GPU    │ Mixed workloads (training │
│                  │ clusters, HPC    │ + serving + web), cloud   │
└──────────────────┴──────────────────┴───────────────────────────┘

Why Meta uses Slurm instead of K8s:
  1. InfiniBand: Slurm has native support. K8s doesn't.
  2. MPI/NCCL: Slurm's srun is designed for tightly-coupled parallel jobs.
     K8s pods are designed for loosely-coupled microservices.
  3. Simplicity: sbatch < 20 lines. K8s YAML = 100+ lines.
  4. Battle-tested at GPU scale: ORNL's Frontier (37K GPUs) runs Slurm.
```

---

## 9. Key Commands

```
  Job submission:
    sbatch train.sh           Submit batch job
    srun python test.py       Run command on allocated nodes
    salloc --gpus=8           Get interactive allocation

  Job management:
    squeue                    Show all queued/running jobs
    squeue -u alice           Show your jobs
    scancel 12345             Kill a job
    scancel -u alice          Kill all your jobs
    scontrol hold 12345       Pause a job (keep in queue)
    scontrol release 12345    Resume a held job

  Cluster status:
    sinfo                     Show partitions and node status
    sinfo -N -l               Show all nodes with details
    scontrol show node node001  Detailed node info

  Job history:
    sacct -j 12345            Show completed job details
    sacct -u alice --starttime=2024-01-01  User history
    sreport cluster utilization  Cluster utilization stats
```
