# Apache Iceberg & Data Lake Table Formats Deep Dive

## Overview

Apache Iceberg is an **open table format** for huge analytic datasets. It sits BETWEEN your query engine (Spark, Trino, Flink) and your storage (S3, GCS, HDFS), adding ACID transactions, schema evolution, time travel, and partition evolution to plain Parquet/ORC files. Think of it as "Git for your data lake" — every write creates a new snapshot, old snapshots are still readable.

## History & Why It Exists

```
The problem (2017):
  Data lakes stored data as directories of Parquet files:
    s3://my-lake/events/year=2024/month=07/part-00001.parquet
    s3://my-lake/events/year=2024/month=07/part-00002.parquet
    ...

  This worked for reads. But writes were BROKEN:

  1. NO TRANSACTIONS:
     Writer A and Writer B both write new files simultaneously.
     Reader sees half of A's files + half of B's → INCONSISTENT.
     No atomic "all or nothing" commit.

  2. NO SCHEMA EVOLUTION:
     Add a new column? Rewrite ALL existing Parquet files.
     Rename a column? Every downstream query breaks.

  3. HIVE PARTITIONING IS RIGID:
     Partitioned by year/month? Want to change to year/month/day?
     Rewrite everything. Terabytes of data movement.

  4. NO TIME TRAVEL:
     "What did the table look like yesterday?"
     Impossible. Old files already overwritten/deleted.

  5. S3 LISTING IS SLOW:
     To query events for July: LIST s3://my-lake/events/year=2024/month=07/
     S3 listing: ~5-50 seconds for 100K files. Before you even READ data.

  Netflix engineers (Ryan Blue et al.) built Iceberg to solve ALL of these.
  Released 2018, Apache top-level project 2020.

Timeline:
  2017  Netflix starts building Iceberg (Ryan Blue)
  2018  Open-sourced, donated to Apache
  2020  Apache top-level project
  2021  Iceberg v1 format spec finalized
  2022  Major adoption: Snowflake, AWS, Databricks (reluctantly)
  2023  Iceberg v2 (row-level deletes, equality deletes)
  2024  Industry convergence: Databricks adopts Iceberg (Unity Catalog),
        Snowflake Polaris Catalog, AWS Glue native Iceberg support
        THE winner of the table format wars

Competitors:
  Delta Lake (Databricks, 2019) — similar goals, Spark-centric
  Apache Hudi (Uber, 2019) — focuses on incremental processing
  Iceberg won because: engine-agnostic (works with everything),
  cleanest specification, no vendor lock-in.
```

---

## 2. What Iceberg Actually Is — Metadata on Top of Files

```
Iceberg is NOT a storage format. It's NOT a query engine.
It's a METADATA LAYER that manages collections of data files.

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                   │
  │  Your data files are still Parquet (or ORC or Avro).             │
  │  Iceberg doesn't change how the data is stored.                  │
  │                                                                   │
  │  What Iceberg ADDS:                                              │
  │    A set of METADATA FILES that describe:                        │
  │    - Which data files belong to the table (manifest lists)       │
  │    - Statistics about each file (min/max, row count)             │
  │    - Schema (column types, IDs for evolution)                    │
  │    - Partition spec (how data is partitioned)                    │
  │    - Snapshots (every version of the table)                      │
  │                                                                   │
  │  ┌──────────────────────────────────────────────────────────┐   │
  │  │                    Table State                             │   │
  │  │                                                           │   │
  │  │  Metadata File (JSON/Avro)                               │   │
  │  │    ├── current snapshot ID: snap-003                     │   │
  │  │    ├── schema: {id: int, name: string, ts: timestamp}   │   │
  │  │    ├── partition spec: day(ts)                           │   │
  │  │    └── snapshot list:                                    │   │
  │  │         snap-001 → manifest-list-001                    │   │
  │  │         snap-002 → manifest-list-002                    │   │
  │  │         snap-003 → manifest-list-003 (current)          │   │
  │  │                                                           │   │
  │  │  Manifest List (snap-003):                               │   │
  │  │    ├── manifest-file-A.avro                              │   │
  │  │    └── manifest-file-B.avro                              │   │
  │  │                                                           │   │
  │  │  Manifest File A:                                        │   │
  │  │    ├── data-00001.parquet (rows: 50K, min_ts: 2024-07-01)│   │
  │  │    ├── data-00002.parquet (rows: 48K, min_ts: 2024-07-01)│   │
  │  │    └── data-00003.parquet (rows: 52K, min_ts: 2024-07-02)│   │
  │  │                                                           │   │
  │  │  Manifest File B:                                        │   │
  │  │    ├── data-00004.parquet (rows: 45K, min_ts: 2024-07-03)│   │
  │  │    └── data-00005.parquet (rows: 51K, min_ts: 2024-07-03)│   │
  │  └──────────────────────────────────────────────────────────┘   │
  │                                                                   │
  │  Data files (Parquet):                                           │
  │    s3://my-lake/data/data-00001.parquet                          │
  │    s3://my-lake/data/data-00002.parquet                          │
  │    ...                                                            │
  └──────────────────────────────────────────────────────────────────┘

The hierarchy:
  Catalog (e.g., Glue, Hive Metastore, Nessie)
    └── Metadata File (one per table, points to current snapshot)
         └── Manifest List (one per snapshot, lists manifest files)
              └── Manifest Files (lists of data files + statistics)
                   └── Data Files (Parquet/ORC on S3/GCS/HDFS)
```

---

## 3. ACID Transactions — How Writes Are Atomic

```
Every write to an Iceberg table creates a NEW SNAPSHOT.
The old snapshot remains unchanged. This is copy-on-write (COW).

  INSERT 1000 rows into table:

  BEFORE:
    Metadata → snap-002 → manifests → [file-A, file-B, file-C]

  WHAT HAPPENS:
    1. Write new Parquet file: file-D.parquet (1000 new rows)
    2. Create new manifest that includes file-D
    3. Create new manifest list: [old-manifests + new-manifest]
    4. Create snap-003 pointing to new manifest list
    5. ATOMIC COMMIT: update metadata pointer from snap-002 → snap-003
       (this is ONE file write — atomic on S3/GCS)

  AFTER:
    Metadata → snap-003 → manifests → [file-A, file-B, file-C, file-D]
    (snap-002 still exists for time travel!)

  If the writer CRASHES between steps 1-4:
    Metadata still points to snap-002. New files are orphaned.
    Table is consistent. No partial state visible.
    Cleanup: periodic garbage collection removes orphaned files.

  ┌──────────────────────────────────────────────────────────────┐
  │  The atomic commit step:                                      │
  │                                                               │
  │  On S3: write new metadata file, then atomically update      │
  │  the "current metadata" pointer in the catalog.              │
  │  Catalog provides the atomicity (Glue, Hive Metastore, etc.)│
  │                                                               │
  │  On HDFS: use rename() which is atomic on HDFS.              │
  │                                                               │
  │  Concurrent writers: optimistic concurrency.                 │
  │  Both read snap-002 → both write new files → both try to     │
  │  commit snap-003. ONE succeeds, the other RETRIES (reads     │
  │  the winner's snap-003, rebases its changes, commits snap-004)│
  └──────────────────────────────────────────────────────────────┘
```

---

## 4. Time Travel — Query Any Version of the Table

```
Every snapshot is kept (until expired):

  snap-001: table as of 2024-07-01 10:00
  snap-002: table as of 2024-07-01 14:00 (added 50K rows)
  snap-003: table as of 2024-07-02 09:00 (deleted some rows)
  snap-004: table as of 2024-07-02 15:00 (updated schema)

  -- Query current state:
  SELECT * FROM events;

  -- Query table as of yesterday:
  SELECT * FROM events FOR SYSTEM_TIME AS OF TIMESTAMP '2024-07-01 14:00:00';

  -- Query by snapshot ID:
  SELECT * FROM events FOR SYSTEM_VERSION AS OF 2;

  -- Rollback to a previous version:
  CALL system.rollback_to_snapshot('events', 2);

Use cases:
  - Debug: "what did the data look like when the dashboard showed wrong numbers?"
  - Audit: "what data was this ML model trained on last week?"
  - Recovery: "someone accidentally deleted rows, roll back"
  - Reproducibility: "re-run last month's report with last month's data"

  Snapshot expiration: configure how long to keep old snapshots.
  Default: often 5-7 days. Expired snapshots → files can be garbage collected.
```

---

## 5. Schema Evolution — Change Schema Without Rewriting Data

```
Iceberg uses COLUMN IDs (not names) internally.
This makes schema changes safe and backward-compatible.

  Original schema:
    id: 1  name: "id"    type: int
    id: 2  name: "name"  type: string
    id: 3  name: "email" type: string

  ADD COLUMN (no rewrite):
    id: 4  name: "age"   type: int
    Old files: don't have "age" → Iceberg returns NULL for age.
    New files: have "age" column.
    No data rewrite needed!

  RENAME COLUMN (no rewrite):
    id: 2  name: "full_name"  type: string  (was "name")
    Column ID 2 is still the same. Just the name changed in metadata.
    Old Parquet files still have the data at column ID 2.
    Query by "full_name" → Iceberg maps to column ID 2 → finds the data.

  DROP COLUMN (no rewrite):
    Remove id: 3 from schema.
    Old files still contain the "email" column, but it's simply ignored.
    Future compaction will physically remove it.

  ┌──────────────────────────────────────────────────────────────┐
  │  Hive/traditional approach:                                   │
  │    Rename column → ALL readers must update. If Parquet files  │
  │    have the old name, queries break. Must rewrite files.      │
  │                                                               │
  │  Iceberg approach:                                            │
  │    Rename column → only metadata changes (column ID stays).  │
  │    Old files work fine. Zero data rewrite. Zero downtime.    │
  └──────────────────────────────────────────────────────────────┘
```

---

## 6. Partition Evolution — Change Partitioning Without Rewriting

```
Traditional (Hive) partitioning:

  CREATE TABLE events PARTITIONED BY (year, month, day);
  → directory layout: /year=2024/month=07/day=15/

  Want to change to hourly partitioning?
  → Must rewrite EVERY file. Petabytes of data movement.

Iceberg hidden partitioning:

  CREATE TABLE events (
    id BIGINT, event_type STRING, ts TIMESTAMP
  ) PARTITIONED BY (day(ts));

  Iceberg computes the partition value FROM the column:
    ts = '2024-07-15 14:30:00' → partition = '2024-07-15'
  Users don't put partition values in their queries.
  Users don't even need to KNOW the table is partitioned.

  -- This just works (no WHERE day = '2024-07-15' needed):
  SELECT * FROM events WHERE ts > '2024-07-15';
  -- Iceberg automatically prunes partitions.

  CHANGE PARTITIONING (no rewrite!):
    ALTER TABLE events ADD PARTITION FIELD hour(ts);

    Old data: still partitioned by day. Not rewritten.
    New data: partitioned by hour.
    Queries work across both: Iceberg checks the partition spec
    of EACH manifest file and plans accordingly.

  ┌──────────────────────────────────────────────────────────────┐
  │  You can even go: monthly → daily → hourly                   │
  │  over time as data grows, WITHOUT rewriting old data.        │
  │  Each snapshot remembers its own partition spec.              │
  │  Iceberg handles the mix transparently.                      │
  └──────────────────────────────────────────────────────────────┘
```

---

## 7. File Pruning — Why Iceberg Queries Are Fast

```
Hive approach: to query, LIST all files in the partition directory on S3.
  100K files? → 5-50 seconds just for listing. Before reading ANY data.

Iceberg approach: read the MANIFEST FILES (metadata).
  Manifests contain: file path + column statistics (min/max) + row count.
  No S3 listing needed. Iceberg knows EXACTLY which files to read.

  Query: SELECT * FROM events WHERE ts > '2024-07-15' AND event_type = 'click'

  Iceberg planning (from metadata, no data read yet):
    1. Partition pruning: skip all partitions before July 15
    2. File pruning: for remaining files, check manifest statistics:
       - file-001: event_type min="click", max="view" → MIGHT match
       - file-002: event_type min="purchase", max="view" → SKIP (no "click")
       - file-003: event_type min="click", max="click" → READ
    3. Read only matching files.

  ┌──────────────────────────────────────────────────────────────┐
  │  Hive: LIST 100K files on S3 (50 sec) + read 10K files      │
  │  Iceberg: read manifest (0.1 sec) + read 500 files          │
  │                                                               │
  │  Iceberg skipped 99.5% of files without touching S3.        │
  │  Planning is 500x faster. I/O is 20x less.                  │
  └──────────────────────────────────────────────────────────────┘
```

---

## 8. Row-Level Operations — UPDATE, DELETE, MERGE

```
Parquet files are IMMUTABLE. You can't update a row in place.
Iceberg handles updates with two strategies:

COPY-ON-WRITE (COW) — default:
  UPDATE events SET status = 'processed' WHERE id = 42;

  1. Find the file containing id=42 (file-A.parquet)
  2. Read ALL rows from file-A
  3. Modify row 42
  4. Write NEW file-A'.parquet (entire file rewritten)
  5. New snapshot: replace file-A with file-A' in manifests

  Pro: read performance unchanged (no extra files to merge).
  Con: expensive writes (rewrite entire file for one row change).
  Best for: infrequent updates, read-heavy workloads.

MERGE-ON-READ (MOR) — Iceberg v2:
  UPDATE events SET status = 'processed' WHERE id = 42;

  1. Write a small DELETE FILE: "in file-A, row 42 is deleted"
  2. Write a small INSERT FILE: "row 42 with status='processed'"
  3. New snapshot includes: file-A + delete-file + insert-file

  At read time: reader merges file-A with delete/insert files.
  Periodic COMPACTION merges everything into clean data files.

  Pro: fast writes (small files, no rewrite).
  Con: slower reads (must merge on the fly until compaction runs).
  Best for: frequent updates, CDC (change data capture) workloads.

  ┌──────────────────────────────────────────────────────────────┐
  │             COW                        MOR                    │
  │  Write cost:  HIGH (rewrite file)    LOW (small delta file)  │
  │  Read cost:   LOW (clean files)      HIGHER (merge at read)  │
  │  Best for:    read-heavy             write-heavy / CDC       │
  └──────────────────────────────────────────────────────────────┘
```

---

## 9. Iceberg vs Delta Lake vs Hudi

```
┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
│                  │ Iceberg          │ Delta Lake       │ Hudi             │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Created by       │ Netflix (2018)   │ Databricks (2019)│ Uber (2019)      │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Transaction log  │ Manifest files   │ JSON log files   │ Timeline metadata│
│                  │ (Avro)           │ (_delta_log/)    │                  │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Engine support   │ All (Spark,Trino,│ Best on Spark.   │ Spark, Flink.    │
│                  │ Flink,Presto,    │ Growing on others│ Limited elsewhere│
│                  │ DuckDB,Snowflake)│                  │                  │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Schema evolution │ Full (column IDs)│ Yes (limited)    │ Yes              │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Partition        │ Hidden + evolve  │ No hidden. Must  │ Limited          │
│ evolution        │ without rewrite  │ rewrite for change│                 │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Time travel      │ Yes (snapshots)  │ Yes (versions)   │ Yes (timeline)   │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Row-level ops    │ COW + MOR (v2)   │ COW + MOR        │ MOR (primary)    │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ File format      │ Parquet, ORC,Avro│ Parquet only     │ Parquet, ORC     │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Catalog          │ Hive, Glue,      │ Unity Catalog,   │ Hive             │
│                  │ Nessie, REST     │ Hive, Glue       │                  │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Vendor lock-in   │ None (open spec) │ Databricks-first │ Uber-first       │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Industry trend   │ WINNING          │ Adopting Iceberg │ Niche            │
│ (2024+)          │                  │ (UniForm compat) │                  │
└──────────────────┴──────────────────┴──────────────────┴──────────────────┘

Why Iceberg is winning:
  1. Engine-agnostic: works with Spark, Trino, Flink, Snowflake, BigQuery, DuckDB
     Delta Lake was Spark-only for years.
  2. Open specification: no single vendor controls it.
  3. Hidden partitioning: users don't need to know partition layout.
  4. Partition evolution: change partitioning without rewriting data.
  5. Industry convergence: Snowflake, AWS, Databricks all support Iceberg now.

Delta Lake response: "UniForm" — Delta files that are ALSO readable as Iceberg.
This is an admission that Iceberg won the format war.
```

---

## 10. The Data Lakehouse Architecture

```
Data Lake (2010s):                  Data Warehouse (traditional):
  ┌──────────────────┐              ┌──────────────────┐
  │ S3 / GCS / HDFS  │              │ Snowflake /      │
  │                  │              │ BigQuery /       │
  │ Cheap storage    │              │ Redshift         │
  │ Any format       │              │                  │
  │ Schema-on-read   │              │ Expensive compute│
  │ NO transactions  │              │ Strong schema    │
  │ NO ACID          │              │ ACID transactions│
  │ NO governance    │              │ Governance       │
  │                  │              │                  │
  │ = messy data swamp│             │ = reliable but   │
  └──────────────────┘              │   costly + siloed│
                                    └──────────────────┘

Data Lakehouse (2020s — Iceberg enables this):
  ┌──────────────────────────────────────────────────────┐
  │                                                       │
  │  S3 / GCS (cheap, open storage)                      │
  │       +                                               │
  │  Iceberg (ACID, schema, time travel, governance)     │
  │       +                                               │
  │  Any engine (Spark, Trino, Flink, DuckDB, Snowflake) │
  │                                                       │
  │  = Lake price + Warehouse reliability + open format  │
  │                                                       │
  │  No data movement between lake and warehouse.        │
  │  One copy of data, multiple engines query it.        │
  └──────────────────────────────────────────────────────┘

  This is why Iceberg matters beyond just "a better Hive":
  It makes the data lake AS RELIABLE as a data warehouse
  while keeping data in open formats on cheap storage.
```

---

## 11. Catalogs — How Engines Find Iceberg Tables

```
A catalog answers: "Where is the current metadata file for table X?"

  ┌──────────────────────────────────────────────────────────────┐
  │ Catalog                                                       │
  │                                                               │
  │  "events" table → s3://my-lake/metadata/v3.metadata.json    │
  │  "users" table  → s3://my-lake/metadata/v5.metadata.json    │
  │                                                               │
  │  Spark, Trino, Flink all ask the catalog:                   │
  │  "Where is the events table?" → get metadata path → read it │
  │  → know which data files to scan.                           │
  └──────────────────────────────────────────────────────────────┘

  Catalog options:
    Hive Metastore:    traditional, widely supported
    AWS Glue:          managed, serverless, AWS-native
    Nessie:            Git-like branching for data (branch/merge tables!)
    REST Catalog:      Iceberg's standard REST API (cloud-native)
    Snowflake Polaris: managed Iceberg catalog by Snowflake (open-sourced)
    Unity Catalog:     Databricks' catalog (supports Iceberg via UniForm)
```

---

## 12. Key Numbers

```
Metadata overhead:      ~KB per manifest entry (file path + stats)
Planning time:          0.1-1 sec (read manifests, prune files)
vs Hive planning:       5-60 sec (S3 file listing for large tables)
Snapshot creation:      milliseconds (write metadata file)
File pruning:           often skips 90-99% of files
Typical data file size: 128 MB - 1 GB (Parquet, compressed)
Concurrent writers:     optimistic concurrency (retry on conflict)
Time travel retention:  configurable (default varies, often 5-7 days)
Supported engines:      Spark, Trino, Flink, Presto, DuckDB, Snowflake,
                        BigQuery, Athena, StarRocks, Doris
Supported storage:      S3, GCS, Azure Blob, HDFS, MinIO
Supported data formats: Parquet (primary), ORC, Avro
```
