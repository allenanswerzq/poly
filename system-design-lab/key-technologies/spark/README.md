# Spark Deep Dive

## Overview

Apache Spark is the **dominant distributed compute engine** for big data. It processes terabytes to petabytes in-memory across a cluster — 10-100x faster than Hadoop MapReduce. Used for batch ETL, SQL analytics, ML training, streaming, and graph processing. If you work with data at scale, you'll use Spark.

## History & Why It Exists

```
The problem (2009):
  Hadoop MapReduce wrote intermediate results to DISK after every stage.
  Iterative algorithms (like ML training) were painfully slow:
    Iteration 1: read from HDFS → map → disk → reduce → disk → HDFS
    Iteration 2: read from HDFS → map → disk → reduce → disk → HDFS
    ...repeat 100 times. Each iteration reads/writes everything to disk.

  Matei Zaharia at UC Berkeley realized: what if we keep data IN MEMORY
  between iterations?

  Answer: RDD (Resilient Distributed Dataset) — an in-memory, fault-
  tolerant, distributed data structure. Keep data in RAM, iterate fast.
  Result: 10-100x faster than MapReduce for iterative workloads.

Timeline:
  2009  Matei Zaharia starts Spark at UC Berkeley AMPLab
  2010  Open-sourced
  2013  Donated to Apache, becomes top-level project
  2014  Spark 1.0 (stable API). Sets world record for large-scale sort.
  2015  DataFrames API (Spark 1.3) — structured, optimized, SQL-like
  2016  Spark 2.0 — Dataset API, Structured Streaming, Catalyst optimizer
  2020  Spark 3.0 — Adaptive Query Execution, GPU support
  2023  Spark 3.5 — Spark Connect (thin client, server-based)
  2024  Spark 4.0 — new streaming engine, Variant type

What Spark replaced:
  MapReduce (batch compute)       → Spark Core
  Hive (SQL analytics)            → Spark SQL
  Mahout (ML on Hadoop)           → MLlib
  Storm (streaming)               → Spark Streaming / Structured Streaming
  Giraph (graph processing)       → GraphX

  One engine replaced 5 specialized tools. That's why Spark won.

Key people:
  Matei Zaharia — creator of Spark, co-founded Databricks
  Databricks — the company behind Spark (raised $10B+, valued at $43B)
  Databricks gives Spark its commercial momentum (managed Spark cloud)
```

## When to Choose Spark

| Use Case | Why Spark |
|----------|---------|
| Batch ETL pipelines | Read TB from S3, transform, write back |
| SQL analytics on data lake | Spark SQL on Parquet/Delta Lake |
| ML feature engineering | Distributed transforms on millions of rows |
| Streaming + batch unified | Structured Streaming uses same API |
| Data engineering | De facto standard tool |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                     Spark Architecture                            │
│                                                                   │
│  ┌──────────────┐                                                │
│  │   Driver      │  Your program runs here.                      │
│  │   Program     │  Creates SparkSession, defines transformations│
│  └──────┬───────┘                                                │
│         │ submits job                                            │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │ Cluster       │  Allocates resources.                         │
│  │ Manager       │  YARN / Kubernetes / Standalone / Mesos       │
│  └──────┬───────┘                                                │
│         │ launches                                               │
│    ┌────┴──────┬──────────────┐                                  │
│    ▼           ▼              ▼                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐                        │
│  │Executor 1 │ │Executor 2 │ │Executor 3 │                        │
│  │ (JVM)     │ │ (JVM)     │ │ (JVM)     │                        │
│  │           │ │           │ │           │                        │
│  │ Task Task │ │ Task Task │ │ Task Task │                        │
│  │ Cache     │ │ Cache     │ │ Cache     │                        │
│  └──────────┘ └──────────┘ └──────────┘                        │
│                                                                   │
│  Driver: plans, schedules, collects results                      │
│  Executors: run tasks, cache data in memory                      │
│  Tasks: smallest unit of work (one partition of data)            │
└──────────────────────────────────────────────────────────────────┘
```

### The Execution Model

```
Your code:
  df = spark.read.parquet("s3://events/")
  result = df.filter(col("year") == 2024) \
             .groupBy("city") \
             .agg(count("*"), avg("value")) \
             .orderBy(desc("count(1)"))
  result.write.parquet("s3://output/")

What Spark does internally:

  1. LOGICAL PLAN (what you asked for):
     Scan → Filter(year=2024) → Aggregate(city) → Sort → Write

  2. CATALYST OPTIMIZER (rewrites for efficiency):
     - Push filter into scan (partition pruning)
     - Column pruning (only read year, city, value)
     - Choose join strategies if joins present

  3. PHYSICAL PLAN (how to execute):
     Stage 1: Scan + Filter + Partial Aggregate (no shuffle)
     ─── shuffle boundary (exchange) ───
     Stage 2: Final Aggregate + Sort + Write

  4. TASK SCHEDULING:
     Stage 1: 200 tasks (one per partition), run across executors
     Stage 2: 200 tasks, each reads shuffled data from Stage 1

  5. EXECUTION:
     Tungsten engine: off-heap memory, code generation,
     whole-stage-codegen (compiles query plan to Java bytecode)
```

## Key Concepts for Interviews

### 1. Lazy Evaluation — Nothing Runs Until Action
```python
# Transformations (lazy — just build the plan):
df = spark.read.parquet("s3://data/")    # lazy
filtered = df.filter(col("x") > 10)      # lazy
grouped = filtered.groupBy("city")        # lazy
result = grouped.count()                  # lazy

# Actions (trigger execution):
result.show()         # NOW Spark actually reads data and computes
result.collect()      # returns results to driver
result.write.parquet("s3://out/")  # writes to storage

# Why lazy?
# Spark sees the ENTIRE chain before executing.
# It can optimize: push filters early, prune columns,
# choose join order, fuse operations.
# Eager execution would miss these optimizations.
```

### 2. Shuffles — The Expensive Operation
```
Shuffle = redistribute data across executors by key.
Needed for: groupBy, join, distinct, orderBy, repartition.

  Executor 1 has: [NYC:5, LA:3, NYC:2]
  Executor 2 has: [LA:7, SF:1, NYC:4]
  Executor 3 has: [SF:9, NYC:1, LA:2]

  After groupBy("city") shuffle:
  Executor A gets all NYC: [5, 2, 4, 1]  → count = 4
  Executor B gets all LA:  [3, 7, 2]     → count = 3
  Executor C gets all SF:  [1, 9]        → count = 2

Why shuffles are expensive:
  - Write intermediate data to disk (shuffle files)
  - Transfer data across the network
  - Must wait for ALL tasks in previous stage to finish

How to minimize shuffles:
  - Filter early (less data to shuffle)
  - Use broadcast joins for small tables (no shuffle on small side)
  - Repartition data once strategically
  - Use bucketed tables (pre-shuffled by key)
```

### 3. Broadcast Joins — Avoid Shuffling Small Tables
```python
# Regular join: shuffle BOTH tables by join key. Expensive.
big.join(small, "id")

# Broadcast join: send small table to ALL executors.
# Each executor has a full copy → no shuffle needed.
from pyspark.sql.functions import broadcast
big.join(broadcast(small), "id")

# Spark auto-broadcasts tables < 10MB (configurable).
# For a 100MB lookup table, explicitly broadcast to save a shuffle.
```

### 4. Caching — Keep Hot Data in Memory
```python
# Without caching: every action re-reads from S3
df = spark.read.parquet("s3://huge/")
df.filter(...).count()    # reads from S3
df.filter(...).show()     # reads from S3 AGAIN

# With caching: read once, store in executor memory
df.cache()                # or df.persist(StorageLevel.MEMORY_AND_DISK)
df.filter(...).count()    # reads from S3, caches
df.filter(...).show()     # reads from MEMORY. Fast!
df.unpersist()            # release when done

# When to cache:
#   ✓ Data used multiple times (iterative ML, multiple queries)
#   ✓ After expensive transformations (post-join, post-aggregate)
#   ✗ Data used once (just read → transform → write)
#   ✗ Data too large for memory (wastes time caching, then spills)
```

### 5. Partitioning — Parallelism Control
```
Partition = smallest unit of data a task processes.
200 partitions → 200 tasks → can run on up to 200 cores.

Too few partitions:  tasks too large, some executors idle, OOM risk
Too many partitions: overhead per task, tiny files written

Rules of thumb:
  - 128MB per partition (similar to HDFS block)
  - 2-4 partitions per CPU core
  - After filter that drops 90% of data → repartition down
  - Before writing: coalesce to target number of output files

# Repartition (full shuffle) vs Coalesce (no shuffle, merge only)
df.repartition(100)    # shuffle to exactly 100 partitions
df.coalesce(10)        # merge partitions without shuffle (reduce only)
```

### 6. Spark SQL & Catalyst Optimizer
```
Catalyst is Spark's query optimizer. It transforms your query plan:

  Original plan:
    Scan(events) → Join(users) → Filter(year=2024) → Project(city, name)

  Optimized plan:
    Scan(events, pushdown: year=2024, columns: [city, user_id])
      → Join(users, broadcast)
        → Project(city, name)

  Optimizations:
    - Predicate pushdown: filter moved into scan
    - Column pruning: only read needed columns
    - Join reordering: small table first
    - Broadcast join: small table auto-broadcast
    - Constant folding: compute constants at plan time
    - Whole-stage codegen: compile stages into Java bytecode
```

### 7. Structured Streaming — Real-Time on Same API
```python
# Batch:
df = spark.read.parquet("s3://events/")
df.groupBy("city").count().write.parquet("s3://counts/")

# Streaming (almost identical code!):
df = spark.readStream.format("kafka") \
     .option("subscribe", "events").load()
df.groupBy("city").count() \
  .writeStream.format("console") \
  .outputMode("complete").start()

# Same DataFrame API for batch and streaming.
# Spark treats a stream as an "unbounded table" that grows.
# Micro-batch processing: every 1-10 seconds, process new data.
# Continuous processing mode available for sub-100ms latency.
```

## Spark vs Alternatives

| Aspect | Spark | Hadoop MR | Presto/Trino | Flink | Pandas |
|--------|-------|-----------|-------------|-------|-------|
| Speed | Fast (in-memory) | Slow (disk) | Fast (pipelined) | Fast (streaming) | Fast (single machine) |
| Best for | Batch ETL + SQL | Legacy batch | Interactive SQL | Stream processing | Small data (<10GB) |
| Programming | Python/Scala/Java/SQL | Java | SQL only | Java/Scala/SQL | Python |
| Streaming | Micro-batch + cont. | No | No | True streaming | No |
| Scale | 1000s of nodes | 1000s of nodes | 100s of nodes | 100s of nodes | 1 machine |
| ML | MLlib | Mahout (dead) | No | FlinkML (early) | scikit-learn |

## Performance Tuning Essentials

```
The top 5 performance killers and fixes:

1. TOO MANY SHUFFLES
   → Filter early, broadcast small tables, pre-bucket data

2. DATA SKEW (one key has 100x more data than others)
   → Salt the key: add random prefix, aggregate twice
   → AQE (Adaptive Query Execution) handles this automatically in Spark 3.0+

3. SMALL FILES (millions of tiny files in S3)
   → Coalesce before writing, use Delta Lake / Iceberg for compaction

4. NOT ENOUGH PARALLELISM
   → Increase partitions, check executor count × cores per executor

5. DRIVER OOM (collecting too much data to driver)
   → Never .collect() large datasets, use .show() or write to storage
```

## Limitations to Mention

- JVM overhead (GC pauses, memory management quirks)
- Not great for sub-second latency (use Flink or Presto for that)
- Python UDFs are slow (data serialized between JVM and Python process)
- Streaming is micro-batch by default, not true event-at-a-time
- Operational complexity on self-managed clusters
- Databricks makes it easy but creates vendor dependency

## Interview Sound Bite

> "For our ETL pipeline, I'd use Spark because we need to process terabytes from S3, join with dimension tables, and write back to Delta Lake. Spark's in-memory processing is 10-100x faster than MapReduce, and the Catalyst optimizer handles predicate pushdown and broadcast joins automatically. For the streaming component, Structured Streaming gives us the same DataFrame API with micro-batch processing."
