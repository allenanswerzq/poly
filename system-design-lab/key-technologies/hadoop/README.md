# Hadoop Deep Dive

## Overview

Hadoop is the **distributed storage and batch processing framework** that started the big data era. It lets you store petabytes of data across thousands of commodity machines (HDFS) and process it in parallel (MapReduce). While MapReduce itself is mostly replaced by Spark, HDFS remains the backbone of most data lakes.

## History & Why It Exists

```
The problem (early 2000s):
  Google had the entire web to index. No single machine could store
  or process that much data. They needed to split data across thousands
  of cheap machines and process it in parallel.

  Google published two papers:
    2003: Google File System (GFS) — distributed storage
    2004: MapReduce — distributed computation

  Doug Cutting (creator of Lucene/Nutch) read these papers and built
  open-source versions:
    HDFS = open-source GFS
    MapReduce = open-source MapReduce
  Named "Hadoop" after his son's toy elephant.

Timeline:
  2003  Google publishes GFS paper
  2004  Google publishes MapReduce paper
  2006  Hadoop becomes Apache top-level project
  2008  Yahoo runs 10,000-node Hadoop cluster
  2011  Hadoop 1.0 released
  2013  Hadoop 2.0 (YARN replaces fixed MapReduce-only scheduler)
  2017  Hadoop 3.0 (erasure coding, GPU support)
  2020s HDFS still everywhere, but MapReduce largely replaced by Spark

What existed before:
  - Expensive proprietary systems (Teradata, Oracle RAC)
  - Vertical scaling (buy bigger machines)
  - No way for startups to process petabytes affordably

What Hadoop changed:
  - Horizontal scaling on COMMODITY hardware (cheap Linux boxes)
  - "Move computation to data" — don't move data, ship code to each node
  - Made petabyte-scale processing accessible to anyone
  - Spawned the entire big data ecosystem (Hive, Pig, HBase, Spark, etc.)
```

## When to Choose Hadoop (HDFS)

| Use Case | Why HDFS |
|----------|---------|
| Data lake storage | Petabyte-scale, cheap, reliable |
| Batch ETL pipelines | Read large datasets sequentially |
| Data archival | Store everything, process later |
| Spark/Hive backend | Most common storage layer for big data tools |

## Architecture

### HDFS — Distributed File System

```
┌──────────────────────────────────────────────────────────────────┐
│                          HDFS Architecture                        │
│                                                                   │
│  Client: "store file.csv (300MB)"                                │
│       │                                                           │
│       ▼                                                           │
│  ┌──────────────┐    Metadata: file → blocks → locations         │
│  │  NameNode     │    "file.csv = [block1, block2, block3]"      │
│  │  (master)     │    "block1 → DataNode 1, 3, 5"               │
│  │              │    "block2 → DataNode 2, 4, 6"               │
│  └──────┬───────┘                                                │
│         │ tells client which DataNodes to write to               │
│         ▼                                                        │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐                │
│  │ DataNode 1 │  │ DataNode 2 │  │ DataNode 3 │ ...            │
│  │ block1     │  │ block2     │  │ block1     │                │
│  │ block3     │  │ block3     │  │ block2     │                │
│  └────────────┘  └────────────┘  └────────────┘                │
│                                                                   │
│  Default: 128MB blocks, 3x replication                           │
│  300MB file → 3 blocks × 3 replicas = 9 block copies            │
│  Any DataNode can fail — data is still on 2 other nodes.        │
└──────────────────────────────────────────────────────────────────┘

Key design decisions:
  - Large blocks (128MB) — optimized for sequential reads, not random access
  - Write-once — files are appended, not edited in place
  - Rack-aware replication — replicas on different racks for fault tolerance
  - NameNode is single point of failure → HA NameNode (active/standby)
```

### MapReduce — The Original Compute Model

```
The idea: break computation into two phases.

  MAP phase:    process each chunk independently (parallel)
  SHUFFLE:      group results by key (framework handles this)
  REDUCE phase: aggregate grouped results (parallel)

Example: count words in 1TB of text

  Input (split across nodes):
    Node 1: "the cat sat on the mat"
    Node 2: "the dog sat on the log"
    Node 3: "the cat and the dog"

  MAP (runs on each node):
    Node 1 → [the:1, cat:1, sat:1, on:1, the:1, mat:1]
    Node 2 → [the:1, dog:1, sat:1, on:1, the:1, log:1]
    Node 3 → [the:1, cat:1, and:1, the:1, dog:1]

  SHUFFLE (framework groups by key, sends to reducers):
    Reducer A gets: the → [1,1,1,1,1]
    Reducer B gets: cat → [1,1], dog → [1,1]
    Reducer C gets: sat → [1,1], on → [1,1]

  REDUCE (aggregate):
    Reducer A: the → 5
    Reducer B: cat → 2, dog → 2
    Reducer C: sat → 2, on → 2

Why MapReduce is slow:
  Every stage writes to DISK (HDFS).
  Map → disk → Shuffle → disk → Reduce → disk
  This is why Spark (in-memory) replaced it — 10-100x faster.
```

### YARN — Resource Manager (Hadoop 2.0+)

```
Before YARN: Hadoop could ONLY run MapReduce.
After YARN:  Hadoop can run ANY framework (Spark, Tez, Flink, etc.)

  ┌───────────────────────────────────────────────────┐
  │                YARN Architecture                    │
  │                                                     │
  │  ┌─────────────────┐                               │
  │  │ ResourceManager  │  Global scheduler              │
  │  │ (one per cluster)│  Allocates containers          │
  │  └────────┬────────┘                               │
  │           │                                         │
  │    ┌──────┴──────┬──────────────┐                  │
  │    ▼             ▼              ▼                   │
  │  ┌──────────┐ ┌──────────┐ ┌──────────┐          │
  │  │NodeManager│ │NodeManager│ │NodeManager│          │
  │  │(per node) │ │(per node) │ │(per node) │          │
  │  │           │ │           │ │           │          │
  │  │ Container │ │ Container │ │ Container │          │
  │  │ (Spark)   │ │ (MapRed)  │ │ (Spark)   │          │
  │  └──────────┘ └──────────┘ └──────────┘          │
  │                                                     │
  │  YARN separated storage (HDFS) from compute.        │
  │  Multiple frameworks can share the same cluster.    │
  └───────────────────────────────────────────────────┘
```

## Key Concepts for Interviews

### 1. Data Locality — "Move Compute to Data"
```
Traditional: data on storage server → copy to compute server → process
Hadoop:      code is small, data is huge → send code to where data lives

When MapReduce schedules a task:
  1st choice: run on node that HAS the data block (data-local)
  2nd choice: run on a node in the same rack (rack-local)
  3rd choice: run on any node (remote — slowest)

This avoids moving terabytes across the network.
```

### 2. Fault Tolerance
```
Node fails during MapReduce job:
  - YARN detects heartbeat timeout
  - Reschedules task on another node
  - Data is on 3 replicas — one of the other nodes has it

NameNode fails:
  - Standby NameNode takes over (HA setup)
  - Uses shared edit log (JournalNodes or NFS)

DataNode fails:
  - NameNode detects missing heartbeats
  - Re-replicates under-replicated blocks to other nodes
```

### 3. The Hadoop Ecosystem
```
HDFS:      distributed storage (the foundation)
YARN:      resource management (runs any framework)
MapReduce: batch compute (legacy, replaced by Spark)
Hive:      SQL on Hadoop (compiles SQL → MapReduce/Tez/Spark jobs)
HBase:     random-access key-value store on HDFS (like BigTable)
Pig:       data flow scripting (deprecated, use Spark)
ZooKeeper: distributed coordination (leader election, config)
Spark:     fast in-memory compute (replaced MapReduce)
Tez:       optimized DAG execution (replaces MapReduce in Hive)
```

## Hadoop vs Alternatives

| Aspect | Hadoop (HDFS + MR) | Spark + HDFS | Cloud Object Store (S3) |
|--------|-------------------|-------------|------------------------|
| Speed | Slow (disk-based) | 10-100x faster (in-memory) | Depends on compute engine |
| Storage cost | Medium (3x replication) | Same (uses HDFS) | Cheapest (no replication mgmt) |
| Operations | Complex (manage cluster) | Complex | Zero (managed) |
| Best era | 2008-2015 | 2015-present | 2018-present |
| Still relevant? | HDFS yes, MapReduce no | Yes (dominant) | Replacing HDFS in cloud |

## Limitations to Mention

- MapReduce is slow — writes to disk between every stage
- NameNode is a bottleneck (all metadata in memory, single machine)
- Small files problem — each file = metadata in NameNode RAM; 1M small files wastes memory
- Not for real-time — batch only (use Kafka + Flink for streaming)
- Operational complexity — managing a Hadoop cluster is a full-time job
- Cloud shift — S3 + Spark/EMR is replacing on-prem HDFS

## Interview Sound Bite

> "HDFS is still the standard distributed file system for data lakes — it partitions data into 128MB blocks across nodes with 3x replication. While MapReduce is legacy, HDFS remains the storage layer for Spark and Hive. The key insight is 'move compute to data' — ship code to nodes rather than moving terabytes across the network."
