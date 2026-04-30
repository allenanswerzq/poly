# MySQL Deep Dive

## Overview

MySQL is the **most widely deployed relational database** in the world. Powers Wikipedia, Facebook (Meta), Twitter (X), and most web applications. In interviews, MySQL and PostgreSQL are often interchangeable — but MySQL has specific strengths in replication and read scaling.

## History & Why It Exists

```
The problem (1990s):
  The web was born. Every website needed a database.
  Oracle and DB2 cost $50K+ per license. PostgreSQL existed but was
  complex and lacked good Windows support and replication.

  Michael "Monty" Widenius created MySQL (1995): a simple, fast,
  free relational database. Named after his daughter "My".
  Philosophy: be FAST and EASY, not feature-complete.

Timeline:
  1995  MySQL 1.0 released (MyISAM engine, no transactions)
  2001  InnoDB storage engine added (ACID transactions!
        Designed by Heikki Tuuri, originally a separate company)
  2005  MySQL 5.0 (stored procedures, triggers, views)
  2008  Sun Microsystems acquires MySQL AB ($1B)
  2009  Oracle acquires Sun → Oracle now owns MySQL
  2009  Monty forks MySQL → MariaDB (community-driven fork)
  2013  MySQL 5.6 (GTID replication, full-text search in InnoDB)
  2016  MySQL 8.0 (window functions, CTEs, JSON improvements)
  2024  MySQL 9.0 (JavaScript stored procedures, vector search)

MySQL vs MariaDB:
  After Oracle acquired MySQL, the original creator forked it.
  MariaDB: community-driven, adds features faster, drop-in compatible.
  MySQL: Oracle-driven, more conservative, but massive install base.
  For interviews, they're interchangeable. Say "MySQL" unless asked.

Key design philosophy:
  - Simple and fast (easy to set up, fast for reads)
  - Pluggable storage engines (InnoDB, MyISAM, Memory, etc.)
  - InnoDB: ACID, row-level locking, MVCC, crash-safe
  - Replication: master-replica is trivially easy to set up
  - The "M" in LAMP stack (Linux + Apache + MySQL + PHP)

Why MySQL dominated the web:
  - Free (GPL license)
  - Easy to install (5 minutes to first query)
  - Fast enough for most web apps
  - Read replicas trivially easy (scale reads horizontally)
  - Every hosting provider supported it

Who uses it:
  Facebook/Meta (largest MySQL deployment: 1000s of shards),
  Wikipedia (entire database), Twitter/X, GitHub, Booking.com,
  Shopify, Uber (MySQL + Vitess for sharding).
```

## When to Choose MySQL

| Use Case | Why MySQL |
|----------|----------|
| Web applications | Battle-tested, massive ecosystem |
| Read-heavy workloads | Excellent replication, read replicas |
| Simple OLTP | Fast for straightforward queries |
| Existing MySQL ecosystem | Vitess, ProxySQL, PlanetScale |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                  MySQL Architecture                      │
│                                                          │
│  Client ──► Connection Handler ──► Thread Pool           │
│                                      │                   │
│                              ┌───────▼────────┐         │
│                              │  Query Parser   │         │
│                              │  Optimizer       │         │
│                              │  Executor        │         │
│                              └───────┬────────┘         │
│                                      │                   │
│                     ┌────────────────┼────────────┐     │
│                     │                │             │     │
│                  InnoDB           MyISAM      Memory    │
│              (default engine)  (legacy, no TX)  (temp)  │
│                     │                                    │
│              ┌──────┴──────┐                            │
│              │ Buffer Pool │  ← Most critical config    │
│              │ (page cache) │                            │
│              └──────┬──────┘                            │
│                     │                                    │
│              ┌──────┴──────┐                            │
│              │ Redo Log     │  ← WAL equivalent          │
│              │ (crash safe) │                            │
│              └─────────────┘                            │
└─────────────────────────────────────────────────────────┘
```

## Key Concepts for Interviews

### 1. InnoDB Storage Engine
- Clustered index on primary key (data stored in PK order)
- MVCC for concurrent reads
- Row-level locking (not table-level like MyISAM)
- Buffer pool: the #1 tuning parameter (set to 70-80% of RAM)

### 2. Replication Topologies
```
Simple replication:
  Primary ──► Replica 1
          ──► Replica 2

Chain replication:
  Primary ──► Replica 1 ──► Replica 2 ──► Replica 3
  (reduces primary load)

Group Replication (multi-primary):
  Node 1 ◄──► Node 2 ◄──► Node 3
  (all can accept writes, conflict detection)
```

### 3. MySQL vs PostgreSQL (Common Interview Question)

| Aspect | MySQL | PostgreSQL |
|--------|-------|------------|
| Replication | More mature, simpler setup | Streaming replication |
| JSON support | JSON type (weaker) | JSONB (indexable, faster) |
| Extensions | Limited | Rich (PostGIS, pg_trgm, etc.) |
| Query optimizer | Simpler, sometimes faster | More sophisticated |
| Community | Enterprise + community | Fully open source |
| Scaling tools | Vitess (YouTube scale) | Citus (distributed) |

### 4. Vitess — MySQL at Scale
- Sharding middleware for MySQL (created at YouTube)
- Horizontal scaling without application changes
- Connection pooling, query routing, schema management
- Used by: Slack, Square, GitHub, PlanetScale

### 5. InnoDB Clustered Index
```
Primary key = clustered index (data sorted by PK)

Auto-increment PK:   sequential inserts → append-only → fast
UUID PK:             random inserts → page splits → slow

Rule: use auto-increment for MySQL, UUID only if you need it
      (or use UUID v7 which is time-ordered)
```

## Scaling MySQL

```
Step 1: Read replicas (80% of read traffic)
Step 2: Connection pooling (ProxySQL)
Step 3: Vertical scaling (more RAM for buffer pool)
Step 4: Table partitioning
Step 5: Vitess or application-level sharding
```

## Limitations to Mention

- Weaker JSONB support than PostgreSQL
- Fewer index types (no GIN/GiST equivalents)
- Schema changes can be slow on large tables (use pt-online-schema-change or gh-ost)
- Group Replication has more overhead than simple primary-replica

## Interview Sound Bite

> "MySQL is great here because we're read-heavy and MySQL's replication is very mature. We'd set up a primary with 3 read replicas behind ProxySQL. If we outgrow a single primary, Vitess gives us horizontal sharding without rewriting the application."
