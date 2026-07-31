# Google BigQuery Deep Dive

## Overview

BigQuery is Google's **fully managed, serverless data warehouse**. No servers to provision, no indexes to tune, no vacuuming. You write SQL, it scans petabytes in seconds using thousands of machines behind the scenes. It's the public cloud version of Google's internal Dremel system — the same system that inspired Apache Parquet.

## History & Why It Exists

```
The lineage:

  2003  Google publishes GFS paper (distributed filesystem)
  2004  Google publishes MapReduce paper
  2006  Google builds Dremel internally
        → interactive SQL over petabytes, columnar, tree-based execution
  2010  Google publishes Dremel paper
        → inspires Apache Parquet, Apache Drill, Impala, Presto
  2011  BigQuery launched as public cloud service
        → Dremel exposed as a product
  2016  BigQuery adds streaming inserts, UDFs, ML (BQML)
  2020  BigQuery Omni (multi-cloud: run on AWS/Azure data)
  2022  BigQuery is the #1 cloud data warehouse by usage
  2024  BigQuery adds vector search, Gemini integration

Why it matters:
  Before BigQuery: to query 1 TB of data, you needed a Hadoop cluster
  (20 machines, Hive, 30 min for a query, ops team to manage).
  BigQuery: same query, 10 seconds, no cluster, pay $5.

Key philosophy:
  - SEPARATE storage and compute (don't pay for idle compute)
  - Serverless (no cluster management, no capacity planning)
  - Pay per query (scan 1 TB = $5, scan 10 GB = $0.05)
  - Columnar everything (Dremel/Capacitor format internally)
```

---

## 2. Architecture — How It Works Under the Hood

```
┌──────────────────────────────────────────────────────────────────────────┐
│                        BigQuery Architecture                              │
│                                                                           │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │  CLIENT                                                            │  │
│  │  SQL query: "SELECT city, COUNT(*) FROM events GROUP BY city"     │  │
│  └──────────────────────────┬─────────────────────────────────────────┘  │
│                              │                                            │
│  ┌──────────────────────────▼─────────────────────────────────────────┐  │
│  │  DREMEL ENGINE (query execution)                                   │  │
│  │                                                                     │  │
│  │  ┌─────────────┐                                                   │  │
│  │  │   Root      │  Receives query, plans execution.                 │  │
│  │  │   Server    │  Rewrites SQL, optimizes.                         │  │
│  │  └──────┬──────┘                                                   │  │
│  │         │                                                           │  │
│  │  ┌──────▼──────────────────────────────────────┐                   │  │
│  │  │  MIXER NODES (intermediate aggregation)      │                  │  │
│  │  │                                              │                   │  │
│  │  │  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐    │                   │  │
│  │  │  │Mixer │  │Mixer │  │Mixer │  │Mixer │    │                   │  │
│  │  │  │  0   │  │  1   │  │  2   │  │  3   │    │                   │  │
│  │  │  └──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘    │                   │  │
│  │  └─────┼──────────┼────────┼──────────┼────────┘                   │  │
│  │        │          │        │          │                              │  │
│  │  ┌─────▼──────────▼────────▼──────────▼────────┐                   │  │
│  │  │  LEAF NODES (parallel scan + filter)         │                  │  │
│  │  │                                              │                   │  │
│  │  │  ┌────┐┌────┐┌────┐┌────┐ ... ┌────┐       │                   │  │
│  │  │  │Leaf││Leaf││Leaf││Leaf│     │Leaf│       │                   │  │
│  │  │  │ 0  ││ 1  ││ 2  ││ 3 │     │ N  │       │                   │  │
│  │  │  └──┬─┘└──┬─┘└──┬─┘└──┬─┘     └──┬─┘       │                   │  │
│  │  └─────┼─────┼─────┼─────┼──────────┼─────────┘                   │  │
│  └────────┼─────┼─────┼─────┼──────────┼─────────────────────────────┘  │
│           │     │     │     │          │                                  │
│  ┌────────▼─────▼─────▼─────▼──────────▼─────────────────────────────┐  │
│  │  COLOSSUS (distributed storage — Google's successor to GFS)        │  │
│  │                                                                     │  │
│  │  Data stored in CAPACITOR format (Google's columnar format):       │  │
│  │    - Column-oriented (like Parquet, but Google's internal version)  │  │
│  │    - Compressed per column (dictionary, RLE, etc.)                 │  │
│  │    - Stored in Colossus (distributed filesystem)                   │  │
│  │    - Replicated for durability                                     │  │
│  │                                                                     │  │
│  │  Storage is SEPARATE from compute:                                 │  │
│  │    Your data sits here whether you're querying or not.             │  │
│  │    You pay for storage ($0.02/GB/month).                           │  │
│  │    Compute spins up ONLY when you run a query.                     │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                                                                           │
│  Also: JUPITER NETWORK (1 Pb/s bisection bandwidth within Google DCs)   │
│    This is why BigQuery can scan petabytes fast — the network between    │
│    storage and compute is incredibly fast (petabit-scale).               │
│                                                                           │
└──────────────────────────────────────────────────────────────────────────┘

The Dremel execution tree:

  Root:   receives query, creates execution plan
     │
  Mixers: intermediate aggregation (reduce data flowing up)
     │
  Leaves: scan storage, apply filters, project columns, partial aggregate
     │
  Storage: Colossus + Capacitor (columnar files)

  This is a TREE, not a pipeline:
    1000s of leaf nodes scan in parallel
    → partial results flow UP through mixers
    → root merges final result
    → return to client

  Each leaf node scans a SLICE of the data (a few column chunks).
  Thousands of leaves = petabyte scan in seconds.
```

---

## 3. Separation of Storage and Compute — The Key Idea

```
Traditional data warehouse (Redshift, on-prem):
  ┌─────────────────────────────────┐
  │  Node 0: CPU + RAM + LOCAL DISK │  Data is ON the node.
  │  Node 1: CPU + RAM + LOCAL DISK │  More data = more nodes.
  │  Node 2: CPU + RAM + LOCAL DISK │  Idle nodes still cost money.
  └─────────────────────────────────┘

  Scaling means: add more nodes (even if you only need more storage
  OR more compute, you get both — coupled, wasteful).

BigQuery (separated):
  ┌──────────────────┐     ┌──────────────────┐
  │  COMPUTE          │     │  STORAGE          │
  │  (Dremel nodes)   │     │  (Colossus)       │
  │                   │     │                   │
  │  Spins up when    │     │  Always on.       │
  │  you run a query. │     │  Pay per GB/month │
  │  Spins down after.│     │  ($0.02/GB/month) │
  │  Pay per TB scanned│     │                   │
  │  ($5/TB)          │     │  10 PB stored =   │
  │                   │     │  $200K/month      │
  │  0 queries = $0   │     │  but $0 compute   │
  └──────────────────┘     └──────────────────┘

  This only works because Google's internal network (Jupiter)
  can move data from Colossus → Dremel nodes at petabit speeds.
  The "remote storage" doesn't feel remote because the network is so fast.

  Snowflake uses the same architecture (S3 for storage, separate compute).
  Redshift Serverless now does too. BigQuery pioneered it.
```

---

## 4. Query Execution — What Happens When You Run SQL

```
Query: SELECT city, COUNT(*), AVG(amount)
       FROM orders
       WHERE date >= '2024-01-01'
       GROUP BY city

Step 1: QUERY PLANNING (root server)
  - Parse SQL → logical plan
  - Optimize: push filters down, prune columns
  - "I need columns: city, amount, date. Ignore the other 50 columns."
  - "Filter date >= 2024-01-01 can skip old partitions entirely."
  - Determine how many leaf nodes needed (~1000 for a 1 TB table)

Step 2: LEAF NODE SCAN (1000s in parallel)
  Each leaf node:
    - Reads a few column chunks from Colossus (only city, amount, date)
    - Applies filter: date >= 2024-01-01 (skip non-matching rows)
    - Partial aggregate: {city: "NYC", count: 42, sum_amount: 50000}
    - Sends partial result UP to mixer

  1000 leaf nodes scanning 1 GB each = 1 TB scanned in ~3 seconds.
  Each leaf only reads 3 columns out of 50 = 94% less I/O.

Step 3: MIXER AGGREGATION (intermediate)
  Mixers receive partial results from ~100 leaves each.
  Merge: combine partial counts and sums by city.
  Send merged results UP to root.

Step 4: ROOT MERGES FINAL RESULT
  Combines results from all mixers.
  Returns: [{city: "NYC", count: 5000, avg: 42.50}, ...]
  to the client.

  ┌────────────────────────────────────────────────────────────┐
  │  Total time: ~5-15 seconds for 1 TB                        │
  │  Cost: 1 TB × $5/TB = $5                                   │
  │  On Hadoop/Hive: same query = 10-30 minutes + cluster cost │
  └────────────────────────────────────────────────────────────┘
```

---

## 5. Storage Format — Capacitor (Google's Columnar Format)

```
Capacitor is Google's internal columnar format (like Parquet, but proprietary).

  Similarities with Parquet:
    - Columnar: each column stored separately
    - Compressed: dictionary, RLE, delta encoding per column
    - Row groups: data split into chunks for parallel reading
    - Statistics: min/max per column per chunk (for predicate pushdown)

  Differences from Parquet:
    - Tighter integration with Colossus (Google's distributed FS)
    - Automatic re-clustering: BigQuery automatically re-sorts data
      over time to improve predicate pushdown (no manual OPTIMIZE)
    - Nested data: uses Dremel encoding (repetition/definition levels)
      — this IS where Parquet got the idea from

  You never interact with Capacitor directly.
  BigQuery abstracts it completely. You just write SQL.
```

---

## 6. Pricing Model — Pay Per Scan

```
TWO pricing models:

  1. ON-DEMAND ($5 per TB scanned):
     - Pay only when you query
     - No commitment, no reservation
     - First 1 TB/month free
     - Great for: infrequent queries, exploration, small teams

  2. CAPACITY (slots — reserved compute):
     - Buy "slots" (virtual CPUs): ~$0.04/slot/hour
     - 100 slots baseline, autoscale to 400
     - Pay regardless of usage (like a reserved cluster)
     - Great for: heavy, predictable workloads

  Why per-TB pricing changes behavior:
    SELECT * FROM big_table;          ← scans ALL columns. Expensive!
    SELECT name, age FROM big_table;  ← scans 2 columns. 50x cheaper!

    This is why BigQuery users learn to:
    - Never SELECT * (scan only needed columns)
    - Partition tables by date (skip old data)
    - Cluster tables by common filter columns
    - Use LIMIT with caution (BigQuery scans BEFORE limiting)

  ┌──────────────────────────────────────────────────────────────┐
  │  Common surprise: "I ran SELECT * LIMIT 10 and it cost $50!" │
  │                                                               │
  │  BigQuery scans the FULL COLUMN before applying LIMIT.       │
  │  LIMIT doesn't reduce scan cost. It only limits output rows. │
  │  To reduce cost: select fewer columns + partition + filter.  │
  └──────────────────────────────────────────────────────────────┘
```

---

## 7. Partitioning & Clustering — Reducing Scan Cost

```
PARTITIONING: physically divide table into segments by a column (usually date).

  CREATE TABLE orders
  PARTITION BY DATE(order_date)
  AS SELECT * FROM raw_orders;

  Stored as:
    orders/date=2024-01-01/  (column chunks for Jan 1)
    orders/date=2024-01-02/  (column chunks for Jan 2)
    ...

  Query: WHERE order_date = '2024-07-15'
  → BigQuery reads ONLY the July 15 partition. Skips everything else.
  → 1/365 of the data scanned. 365x cost reduction.


CLUSTERING: sort data WITHIN each partition by specified columns.

  CREATE TABLE orders
  PARTITION BY DATE(order_date)
  CLUSTER BY city, customer_id
  AS SELECT * FROM raw_orders;

  Within each date partition, data is sorted by city then customer_id.
  Query: WHERE city = 'NYC' → min/max statistics are tight
  → BigQuery skips blocks where city isn't 'NYC'.
  → Additional 5-50x cost reduction on top of partitioning.

  ┌──────────────────────────────────────────────────────────────┐
  │  Without partitioning + clustering:  scan 10 TB = $50       │
  │  With date partitioning:             scan 27 GB  = $0.14    │
  │  With clustering on city:            scan 5 GB   = $0.025   │
  │                                                               │
  │  Same query, same result. 2000x cheaper.                    │
  └──────────────────────────────────────────────────────────────┘
```

---

## 8. Streaming Inserts & Real-Time

```
Two ways to load data into BigQuery:

  1. BATCH LOAD (free!):
     Load from GCS (Parquet, CSV, JSON) → BigQuery.
     Processed as a batch job. Free. Takes seconds to minutes.
     Best for: ETL pipelines, scheduled loads, historical data.

  2. STREAMING INSERT (paid: $0.01/200 MB):
     Insert rows via API in real-time.
     Available for query within seconds.
     Used for: real-time dashboards, event streams, log analytics.

     Internally: streaming data goes into a BUFFER first,
     then periodically merged into Capacitor columnar storage.
     Buffer is queryable immediately but not yet optimized.

  3. BigQuery Storage Write API (newer, recommended):
     Stream data in Arrow or Protobuf format.
     Higher throughput than streaming insert API.
     Exactly-once semantics (vs at-least-once for old API).

  ┌──────────────────────────────────────────────────────────────┐
  │  Common pattern:                                              │
  │    Pub/Sub → Dataflow (Apache Beam) → BigQuery streaming     │
  │    Events flow in real-time. Queryable within seconds.       │
  │    Dashboard refreshes every 30 seconds with live data.      │
  └──────────────────────────────────────────────────────────────┘
```

---

## 9. BigQuery ML (BQML) — ML Without Moving Data

```
Train ML models directly in BigQuery with SQL:

  CREATE MODEL my_model
  OPTIONS(model_type='logistic_reg')
  AS
  SELECT city, age, amount, purchased
  FROM training_data;

  -- Predict:
  SELECT * FROM ML.PREDICT(MODEL my_model,
    (SELECT city, age, amount FROM new_data));

  Supported models:
    - Linear/logistic regression
    - K-means clustering
    - Matrix factorization (recommendations)
    - Time-series forecasting (ARIMA)
    - Deep neural networks (via TensorFlow integration)
    - XGBoost
    - Imported TensorFlow/ONNX models

  Why this matters:
    Traditional ML: export TB of data from warehouse → load into
    Python → train → export model → import predictions back.
    BQML: data never leaves BigQuery. Train where the data lives.
```

---

## 10. BigQuery vs Snowflake vs Redshift vs Databricks

```
┌──────────────────┬──────────────┬──────────────┬──────────────┬──────────────┐
│                  │ BigQuery     │ Snowflake    │ Redshift     │ Databricks   │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Cloud            │ GCP only     │ Multi-cloud  │ AWS only     │ Multi-cloud  │
│                  │ (+ Omni)     │              │              │              │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Architecture     │ Serverless   │ Separated    │ Was coupled, │ Lakehouse    │
│                  │ (no cluster) │ storage/comp │ now serverless│ (lake + wh) │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Pricing          │ Per TB scan  │ Per compute  │ Per node or  │ Per compute  │
│                  │ or slots     │ time (credits)│ serverless   │ time (DBU)   │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Storage format   │ Capacitor    │ Micro-       │ Custom       │ Delta Lake   │
│                  │ (columnar)   │ partitions   │ columnar     │ (Parquet)    │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Best for         │ GCP users,   │ Multi-cloud, │ AWS users,   │ ML + data    │
│                  │ ad-hoc SQL,  │ data sharing │ existing     │ engineering  │
│                  │ serverless   │ concurrency  │ Redshift     │ Spark users  │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ ML integration   │ BQML (SQL)   │ Snowpark     │ Redshift ML  │ MLflow,      │
│                  │              │ (Python/Java)│ (SageMaker)  │ native       │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Streaming        │ Yes (native) │ Snowpipe     │ Kinesis      │ Structured   │
│                  │              │ (micro-batch)│ integration  │ Streaming    │
├──────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ Open format      │ No           │ No (Iceberg  │ No           │ Yes (Delta/  │
│                  │ (Capacitor)  │ support)     │ (proprietary)│ Parquet)     │
└──────────────────┴──────────────┴──────────────┴──────────────┴──────────────┘

BigQuery's advantages:
  - True serverless (zero cluster management, zero capacity planning)
  - Per-query pricing (pay $5 for a 1 TB query, nothing when idle)
  - Petabyte-scale without config (just works)
  - GCP ecosystem integration (Pub/Sub, Dataflow, Vertex AI)

BigQuery's disadvantages:
  - GCP only (Omni exists but limited)
  - Proprietary format (data locked in Capacitor, not Parquet)
  - Per-TB pricing can surprise you (SELECT * is expensive)
  - Less flexible than Databricks for non-SQL workloads (Python/Spark)
```

---

## 11. Key Numbers

```
Query performance:
  1 TB scan:              ~5-15 seconds
  10 TB scan:             ~15-30 seconds
  100 TB scan:            ~30-60 seconds
  Concurrent queries:     up to 100 (on-demand), 2000+ (slots)

Pricing (on-demand, 2024):
  Query:                  $5 per TB scanned (first 1 TB/month free)
  Storage:                $0.02/GB/month (active)
                          $0.01/GB/month (long-term, >90 days untouched)
  Streaming insert:       $0.01 per 200 MB

Limits:
  Max columns per table:  10,000
  Max row size:           100 MB
  Max query result:       ~128 MB (for interactive), larger for exports
  Max query runtime:      6 hours (default)
  Max concurrent slots:   2000+ (auto-scaling with reservations)

Storage:
  Format:                 Capacitor (columnar, compressed, Dremel-based)
  Replication:            Automatic, multi-zone
  Encryption:             At rest + in transit (default, AES-256)
```
