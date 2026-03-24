# ClickHouse Deep Dive

## Overview

ClickHouse is a **columnar OLAP database** built for analytics. It can scan billions of rows per second on a single node. Choose it when you need fast aggregation queries (COUNT, SUM, AVG) over massive datasets — dashboards, metrics, ad analytics.

## When to Choose ClickHouse

| Use Case | Why ClickHouse |
|----------|--------------|
| Analytics dashboards | Aggregate billions of rows in seconds |
| Ad click tracking | Count clicks by campaign/region/time |
| Log analysis | Fast search + aggregation on log data |
| Metrics monitoring | Time-series aggregation |
| A/B test analysis | Fast GROUP BY over experiment data |

## Why Columnar Storage Matters

```
Row-oriented (PostgreSQL):         Column-oriented (ClickHouse):
Store all columns of a row         Store each column separately
together on disk.                  on disk.

┌──────┬──────┬────────┐          ┌──────┬──────┬──────┬──────┐
│ id=1 │ NYC  │  29.99 │          │ id=1 │ id=2 │ id=3 │ id=4 │  ← id column
│ id=2 │ LA   │   9.99 │          └──────┴──────┴──────┴──────┘
│ id=3 │ NYC  │  49.99 │          ┌──────┬──────┬──────┬──────┐
│ id=4 │ SF   │  19.99 │          │ NYC  │  LA  │ NYC  │  SF  │  ← city column
└──────┴──────┴────────┘          └──────┴──────┴──────┴──────┘
                                  ┌──────┬──────┬──────┬──────┐
Query: SELECT AVG(price)          │29.99 │ 9.99 │49.99 │19.99 │  ← price column
  Row store: read ALL columns     └──────┴──────┴──────┴──────┘
  of ALL rows (wasted I/O)
                                  Query: SELECT AVG(price)
                                    Only read the price column!
                                    Skip id and city entirely.
                                    + Same-type data compresses 10-100x
```

## Key Concepts for Interviews

### 1. MergeTree Engine (The Heart of ClickHouse)
```sql
CREATE TABLE events (
    event_date Date,
    user_id UInt64,
    event_type String,
    value Float64
) ENGINE = MergeTree()
ORDER BY (event_date, user_id)  -- sort key (like clustered index)
PARTITION BY toYYYYMM(event_date);  -- monthly partitions
```
- Data sorted by ORDER BY key → range queries are fast
- Partitions → only scan relevant months
- Background merges consolidate small files

### 2. Approximate Queries (Trade Accuracy for Speed)
```sql
-- Exact count distinct: slow on 1B rows
SELECT COUNT(DISTINCT user_id) FROM events;  -- 30 seconds

-- Approximate: uniqHLL12 (HyperLogLog, <1% error)
SELECT uniqHLL12(user_id) FROM events;       -- 0.5 seconds

-- Sample: scan only 10% of data
SELECT AVG(value) FROM events SAMPLE 0.1;    -- 10x faster
```

### 3. Materialized Views (Pre-Aggregate on Insert)
```sql
-- Raw events: 1 billion rows
CREATE TABLE raw_events (...) ENGINE = MergeTree();

-- Pre-aggregated: count per hour per event_type
CREATE MATERIALIZED VIEW hourly_counts
ENGINE = SummingMergeTree() ORDER BY (hour, event_type)
AS SELECT
    toStartOfHour(timestamp) AS hour,
    event_type,
    count() AS cnt
FROM raw_events
GROUP BY hour, event_type;

-- Query the materialized view: milliseconds instead of minutes
SELECT * FROM hourly_counts WHERE hour > now() - INTERVAL 24 HOUR;
```

### 4. Performance Numbers
```
Single node, commodity hardware:
  Scan speed:      1-2 billion rows/second
  Compression:     10-100x (same-type columns compress well)
  Insert speed:    1-2 million rows/second (batched)
  Storage cost:    ~10x less than row-oriented (compression)

Typical: 1TB of raw data → ~100GB in ClickHouse
```

## ClickHouse vs Other Solutions

| Aspect | ClickHouse | PostgreSQL | Elasticsearch | BigQuery |
|--------|-----------|-----------|--------------|---------|
| Query type | OLAP (aggregation) | OLTP + OLAP | Full-text search | OLAP |
| Scan speed | Billions/sec | Millions/sec | Millions/sec | Billions/sec |
| Real-time insert | Yes (batched) | Yes | Yes | No (batch load) |
| Cost | Self-hosted | Self-hosted | Self-hosted | Pay per query |
| JOINs | Limited | Full | None | Full |
| Best for | Analytics | Transactions | Search | Ad-hoc analytics |

## Limitations to Mention

- Not for OLTP (single-row updates are slow)
- No UPDATE/DELETE at row level (use ALTER TABLE + mutations)
- JOINs are limited (best to denormalize or use subqueries)
- INSERT must be batched (don't insert row-by-row)
- Eventual consistency on replicas

## Interview Sound Bite

> "For the analytics dashboard, I'd use ClickHouse because we're doing COUNT/SUM/AVG over billions of ad impressions. Columnar storage means we only read the columns we need, with 10-100x compression. We'd partition by date and use materialized views to pre-aggregate hourly metrics, giving us sub-second dashboard queries."
