# Data Architecture — Warehouse vs Lake vs Lakehouse

## Overview

Understanding the evolution from data warehouse → data lake → lakehouse is essential context for every data/infra topic. This doc covers the WHY behind each architecture, when each won, and why lakehouse is the 2024+ default.

---

## 1. Data Warehouse (2000s — Traditional)

```
"An organized library where every book is cataloged before shelving."

  ┌─────────────────────────────────────────────────────────────┐
  │                     DATA WAREHOUSE                           │
  │                                                              │
  │  Source systems ──► ETL ──► Warehouse ──► BI / Dashboards   │
  │  (MySQL, APIs,    (clean,   (Snowflake,   (Tableau,         │
  │   Salesforce)      transform, BigQuery,    Looker,          │
  │                    load)      Redshift,    PowerBI)          │
  │                              Teradata)                       │
  │                                                              │
  │  Key properties:                                             │
  │    • Schema-on-WRITE: define columns, types BEFORE loading  │
  │    • Structured data ONLY (rows + columns, SQL)             │
  │    • ACID transactions (no partial loads)                    │
  │    • Optimized for queries (indexes, materialized views)    │
  │    • Proprietary storage format (vendor-specific)           │
  │    • Expensive ($10-100/TB/month vs $0.02/GB on S3)         │
  │    • Pre-computed aggregations for fast dashboards           │
  └─────────────────────────────────────────────────────────────┘

How ETL works:
  Extract:   pull data from source systems (MySQL, APIs, CSV files)
  Transform: clean, deduplicate, join, aggregate, apply business rules
  Load:      insert into warehouse in the defined schema

  ETL runs on a SCHEDULE (nightly, hourly).
  Data in the warehouse is always BEHIND real-time by the ETL lag.

  ┌──────────────────────────────────────────────────────────┐
  │  MySQL (OLTP) ──nightly ETL──► Snowflake (OLAP)          │
  │  "operational"                  "analytical"              │
  │                                                           │
  │  OLTP: fast single-row reads/writes (serve the app)      │
  │  OLAP: fast scans/aggregations (serve the analyst)       │
  │  Different workloads → different systems.                │
  └──────────────────────────────────────────────────────────┘

Who used it:
  Every enterprise. Teradata (1990s), Oracle DW, SQL Server SSAS.
  Then cloud: Redshift (2012), BigQuery (2011), Snowflake (2014).

What it's good at:
  ✓ Fast, reliable SQL queries for business analysts
  ✓ Strong governance (who can see what data)
  ✓ Consistent, curated data (one source of truth)
  ✓ BI tools love it (Tableau, Looker, PowerBI)

What it's bad at:
  ✗ EXPENSIVE for large data (PB = millions $/year)
  ✗ Only structured data (no logs, images, ML training data)
  ✗ Rigid schema (change schema = painful migration)
  ✗ Vendor lock-in (data in proprietary format)
  ✗ ETL lag (data is hours/days old)
  ✗ Only a SUBSET of data makes it in (raw data discarded)
```

---

## 2. Data Lake (2010s — The Hadoop/S3 Era)

```
"A storage unit where you dump everything. Organize it later (maybe)."

  ┌─────────────────────────────────────────────────────────────┐
  │                       DATA LAKE                              │
  │                                                              │
  │  Source systems ──► Ingest (raw) ──► S3/HDFS ──► Query      │
  │  (MySQL, APIs,      (Kafka, Flume,  (Parquet,    (Spark,    │
  │   logs, IoT,        Sqoop, just     CSV, JSON,   Hive,     │
  │   images, events)   dump it!)       Avro, raw)   Presto,   │
  │                                                   Athena)   │
  │                                                              │
  │  Key properties:                                             │
  │    • Schema-on-READ: dump data first, define schema at query│
  │    • ANY data type (structured, semi-structured, raw binary)│
  │    • CHEAP storage (S3: $0.023/GB/month, vs $10+/TB in DW) │
  │    • Open formats (Parquet, ORC, JSON, CSV, Avro)           │
  │    • No vendor lock-in (data is on S3 in open formats)      │
  │    • No transactions (concurrent writes can corrupt)         │
  │    • No governance by default (data swamp risk)              │
  └─────────────────────────────────────────────────────────────┘

Why data lakes emerged:
  1. COST: storing 100 TB in Teradata = $1M+/year. On S3 = $2,300/year.
  2. DATA VARIETY: ML needs raw text, images, audio. DW can't store these.
  3. HADOOP HYPE: "store everything, process with MapReduce" (2008-2015).
  4. DATA SCIENCE: "give me ALL the data, I'll find patterns."

The Hadoop era (2008-2018):
  HDFS (storage) + MapReduce/Spark (compute) + Hive (SQL on Hadoop)
  = the original "data lake" stack.

  Then S3 replaced HDFS for most companies:
    HDFS: must run your own cluster (ops burden, capacity planning).
    S3: managed, infinitely scalable, $0.023/GB. Just upload files.

The DATA SWAMP problem:
  ┌──────────────────────────────────────────────────────────────┐
  │  What was SUPPOSED to happen:                                 │
  │    "Store everything → data scientists discover insights!"    │
  │                                                               │
  │  What ACTUALLY happened:                                      │
  │    "Store everything → nobody knows what's in there."        │
  │                                                               │
  │  • 10,000 Parquet files. Which ones are current?             │
  │  • Schema changed 6 months ago. Half the files have old schema│
  │  • Two teams wrote to the same path. Data is mixed/corrupt. │
  │  • "Where is the sales table?" "Which S3 path?" "Dunno."    │
  │  • No access control. Intern can read PII data.              │
  │  • Query returns wrong results. No way to verify consistency.│
  │                                                               │
  │  The lake became a SWAMP.                                    │
  └──────────────────────────────────────────────────────────────┘

Result: most companies ended up with BOTH:
  Data Lake (S3)         → raw data, cheap, messy
       │
       │ ETL (Spark jobs)
       ▼
  Data Warehouse (Snowflake) → curated data, fast queries, expensive

  TWO copies of data. ETL in between. Lag. Inconsistency. Complexity. $$$.
```

---

## 3. Data Lakehouse (2020s — The Convergence)

```
"What if the lake had warehouse features? One copy, best of both."

  ┌─────────────────────────────────────────────────────────────────┐
  │                       DATA LAKEHOUSE                             │
  │                                                                  │
  │  Source systems ──► Ingest ──► S3/GCS ──► Query (any engine)    │
  │                                  │                                │
  │                     ┌────────────▼────────────┐                  │
  │                     │  TABLE FORMAT            │                  │
  │                     │  (Iceberg / Delta Lake)  │                  │
  │                     │                          │                  │
  │                     │  Adds:                   │                  │
  │                     │  • ACID transactions     │                  │
  │                     │  • Schema evolution      │                  │
  │                     │  • Time travel           │                  │
  │                     │  • Partition evolution   │                  │
  │                     │  • File-level statistics │                  │
  │                     │  • Governance metadata   │                  │
  │                     └────────────┬─────────────┘                  │
  │                                  │                                │
  │                     ┌────────────▼────────────┐                  │
  │                     │  Parquet files on S3/GCS │                  │
  │                     │  (open format, cheap)    │                  │
  │                     └──────────────────────────┘                  │
  │                                                                  │
  │  Query with ANY engine:                                          │
  │    Spark, Trino, Flink, DuckDB, Snowflake, BigQuery, Athena    │
  │                                                                  │
  │  Result:                                                         │
  │    Lake storage price ($0.02/GB/month)                          │
  │  + Warehouse reliability (ACID, schema, governance)             │
  │  + Open format (no vendor lock-in, any engine)                  │
  │  + ONE copy of data (no ETL between lake and warehouse)         │
  └─────────────────────────────────────────────────────────────────┘

What made lakehouse possible:
  1. TABLE FORMATS (Iceberg, Delta Lake, Hudi):
     Added ACID transactions + metadata to plain Parquet files.
     Now writes are safe, reads are consistent, schema can evolve.

  2. FAST QUERY ENGINES (Trino, DuckDB, DataFusion):
     Can query Parquet on S3 at near-warehouse speed.
     Columnar format + predicate pushdown + column pruning.

  3. CATALOGS (Glue, Nessie, Unity, Polaris):
     "Where is the sales table?" → catalog knows.
     Governance: access control, audit logs, lineage.

  4. SEPARATION OF STORAGE AND COMPUTE:
     Storage = S3 (cheap, infinite). Always on.
     Compute = Spark/Trino (spin up when querying). Pay per use.
     Don't pay for idle compute. Scale independently.
```

---

## 4. The Full Comparison

```
┌────────────────────────┬───────────────────┬───────────────────┬───────────────────┐
│                        │ Data Warehouse     │ Data Lake          │ Data Lakehouse     │
├────────────────────────┼───────────────────┼───────────────────┼───────────────────┤
│ Era                    │ 2000s-present      │ 2010s-present      │ 2020s-present      │
│ Storage                │ Proprietary        │ S3/GCS (open)      │ S3/GCS (open)      │
│ Format                 │ Vendor-specific    │ Parquet/CSV/JSON   │ Parquet + Iceberg  │
│ Schema                 │ On write (strict)  │ On read (loose)    │ On write (flexible)│
│ ACID transactions      │ Yes                │ No                 │ Yes (Iceberg)      │
│ Data types             │ Structured only    │ Anything           │ Anything           │
│ Query speed            │ Fast (seconds)     │ Slow-medium        │ Fast (seconds)     │
│ Cost (storage)         │ $$$                │ $                  │ $                  │
│ Cost (compute)         │ Always-on cluster  │ Pay per job        │ Pay per query      │
│ Schema evolution       │ Painful (migrate)  │ No enforcement     │ Easy (Iceberg)     │
│ Time travel            │ Some (limited)     │ No                 │ Yes (snapshots)    │
│ Governance             │ Built-in           │ DIY                │ Catalog-based      │
│ Vendor lock-in         │ High               │ Low                │ Low                │
│ ML/unstructured data   │ No                 │ Yes                │ Yes                │
│ BI/analyst queries     │ Excellent          │ OK                 │ Good-Excellent     │
│                        │                    │                    │                    │
│ Examples               │ Snowflake,BigQuery │ S3+Hive, HDFS      │ S3+Iceberg+Trino  │
│                        │ Redshift,Teradata  │ +Spark             │ Databricks Delta   │
└────────────────────────┴───────────────────┴───────────────────┴───────────────────┘
```

---

## 5. The Modern Stack (2024+)

```
A typical modern data platform:

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                   │
  │  DATA SOURCES                                                    │
  │  ├── Transactional DBs (PostgreSQL, MySQL) ──► CDC (Debezium)   │
  │  ├── Event streams (Kafka) ──► streaming ingestion              │
  │  ├── APIs (REST, webhooks) ──► batch/micro-batch                │
  │  └── Files (CSV, JSON uploads) ──► bulk load                    │
  │                                      │                           │
  │                          ┌───────────▼───────────┐              │
  │                          │   INGESTION LAYER      │              │
  │                          │   Kafka / Flink /       │              │
  │                          │   Spark Streaming       │              │
  │                          └───────────┬───────────┘              │
  │                                      │                           │
  │                          ┌───────────▼───────────┐              │
  │                          │   STORAGE LAYER        │              │
  │                          │                        │              │
  │                          │   S3 / GCS             │              │
  │                          │   + Parquet files       │              │
  │                          │   + Iceberg metadata    │              │
  │                          └───────────┬───────────┘              │
  │                                      │                           │
  │                          ┌───────────▼───────────┐              │
  │                          │   CATALOG              │              │
  │                          │   (Glue / Nessie /     │              │
  │                          │    Polaris / Unity)     │              │
  │                          │   "Where is each table?"│              │
  │                          └───────────┬───────────┘              │
  │                                      │                           │
  │               ┌──────────────────────┼──────────────────────┐   │
  │               │                      │                      │   │
  │      ┌────────▼────────┐   ┌────────▼────────┐   ┌────────▼───┐│
  │      │  SQL / BI        │   │  Data Eng        │   │  ML / AI   ││
  │      │                  │   │                  │   │            ││
  │      │  Trino/Presto    │   │  Spark           │   │  PyTorch   ││
  │      │  DuckDB          │   │  dbt             │   │  Ray       ││
  │      │  Athena          │   │  Airflow         │   │  training  ││
  │      │  Tableau/Looker  │   │  Dagster         │   │  datasets  ││
  │      └─────────────────┘   └─────────────────┘   └────────────┘│
  │                                                                   │
  │  ALL reading from the SAME data on S3 through the SAME catalog.  │
  │  One copy. Multiple consumers. Open formats. No ETL between.     │
  └──────────────────────────────────────────────────────────────────┘
```

---

## 6. When to Use What (Decision Guide)

```
"I'm an analyst who needs fast dashboards"
  → Data Warehouse (Snowflake / BigQuery). Zero ops. Fast SQL.
  → Don't overthink it. Pick one and go.

"I'm building a new data platform from scratch"
  → Lakehouse (S3 + Iceberg + Trino/Spark). Future-proof. Open.
  → Add dbt for transformations, Airflow/Dagster for orchestration.

"I have PBs of raw data (logs, events, ML training)"
  → Data Lake on S3 with Iceberg for the curated layer.
  → Raw zone (dump everything) + curated zone (Iceberg tables).

"I already have Snowflake/BigQuery and it works fine"
  → Keep it. Don't migrate for the sake of architecture trends.
  → Consider lakehouse only when: cost is too high, vendor lock-in
    is a problem, or ML team needs raw data access.

"I need real-time analytics (not batch)"
  → Kafka → Flink → Iceberg (streaming into lakehouse).
  → Or: Kafka → ClickHouse (real-time OLAP, separate from lake).
  → Or: Kafka → Snowflake Snowpipe (streaming into warehouse).

"I'm a startup with <10 TB"
  → PostgreSQL for everything. Add a warehouse when you outgrow it.
  → Don't build a lakehouse at 10 TB. It's over-engineering.
```

---

## 7. Key Terms Glossary

```
┌────────────────────┬──────────────────────────────────────────────────┐
│ Term               │ What it means                                    │
├────────────────────┼──────────────────────────────────────────────────┤
│ OLTP               │ Online Transaction Processing. Serve the app.   │
│                    │ Fast single-row ops. PostgreSQL, MySQL.          │
│ OLAP               │ Online Analytical Processing. Serve the analyst.│
│                    │ Fast scans, aggregations. BigQuery, Snowflake.  │
│ ETL                │ Extract, Transform, Load. Move data between sys.│
│ ELT                │ Extract, Load, Transform. Load raw, transform   │
│                    │ inside the warehouse (dbt does this).            │
│ CDC                │ Change Data Capture. Stream DB changes to lake. │
│                    │ Debezium reads MySQL/PG WAL → Kafka → lake.    │
│ Schema-on-write    │ Define schema BEFORE loading. Warehouse style.  │
│ Schema-on-read     │ Dump data first. Define schema at query time.   │
│ Table format       │ Metadata layer on files. Iceberg, Delta, Hudi.  │
│ Catalog            │ Registry of tables. "Where is the sales table?" │
│ Medallion arch.    │ Bronze (raw) → Silver (cleaned) → Gold (curated)│
│                    │ Common data lake organization pattern.           │
│ Data mesh          │ Decentralized ownership. Each team owns their   │
│                    │ data as a product. Federated governance.         │
└────────────────────┴──────────────────────────────────────────────────┘
```
