# SQLite Deep Dive

## Overview

SQLite is an **embedded database** — the entire database is a single file. No server, no configuration, no network. It's the most deployed database in the world (every phone, every browser, every desktop app). In system design, know it for edge/embedded scenarios and as a learning tool for how SQL databases work internally.

## History & Why It Exists

```
The problem (2000):
  D. Richard Hipp was building software for the US Navy on a
  battleship destroyer. The system used Informix as the database.
  When Informix's server went down, the software stopped working.

  Hipp thought: "Why do I need a client-server database? My program
  is the only thing reading and writing this data. What if the
  database was just a LIBRARY linked into my application?"

  He built SQLite: a complete SQL database engine in a single C file.
  No server process. No configuration. No network. Database = one file.
  Link the library, call sqlite3_open(), and you have a full RDBMS.

Timeline:
  2000  SQLite 1.0 (D. Richard Hipp, inspired by PostgreSQL's grammar)
  2001  SQLite adopted by PHP, Python standard library
  2004  SQLite 3.0 (complete rewrite, the version everyone uses today)
  2005  Adopted by Apple (every iPhone), Google (every Android phone)
  2010  SQLite in every major browser (Web SQL, IndexedDB backends)
  2016  WAL mode matures (concurrent readers + writer)
  2021  Litestream (streaming replication for SQLite)
  2023  Turso/libSQL (fork for distributed edge deployment)
  2024  SQLite as edge database renaissance (Cloudflare D1, Fly.io LiteFS)

SQLite by the numbers:
  - ~1 trillion SQLite databases in active use (estimate)
  - Every iPhone, iPad, Mac, Android phone, Windows 10+ machine
  - Every Chrome, Firefox, Safari browser
  - Every Python, PHP installation
  - Most deployed software component of any kind in the world

Key design philosophy:
  - Serverless: no separate process, no admin needed
  - Zero-config: no setup, no tuning, no users/permissions
  - Single file: entire database = one file on disk, easy to copy/backup
  - ACID compliant: yes, SQLite is fully transactional
  - Cross-platform: same database file works on every OS/architecture
  - Public domain: no license at all (not even MIT/BSD)

When SQLite makes sense (and when it doesn't):
  ✓ Embedded applications, mobile apps, desktop apps
  ✓ Testing (use SQLite in tests, PostgreSQL in prod)
  ✓ Edge computing (Cloudflare Workers, Fly.io)
  ✓ Single-user or low-concurrency applications
  ✗ High write concurrency (single writer at a time)
  ✗ Multi-machine access (it's a local file)
  ✗ Large datasets (>1TB gets awkward)
```

## When to Choose SQLite

| Use Case | Why SQLite |
|----------|----------|
| Mobile apps | No server needed, just a file |
| Desktop apps | Electron, VS Code settings, browsers |
| Edge/IoT devices | Tiny footprint (~600KB) |
| Testing | In-memory mode, no setup |
| Small websites | <100K requests/day is fine |
| Data files | Replace CSV/JSON with queryable format |

## Architecture (Simplicity Is the Feature)

```
PostgreSQL:                          SQLite:
  Client ──network──► Server          Application
  Server ──process──► Storage           │
  Connection pool                       │ direct function call
  WAL management                        │ (no network, no IPC)
  Vacuum daemon                         ▼
  Replication                        ┌──────────┐
  Auth & permissions                 │ SQLite    │
  ...much complexity                 │ Library   │
                                     └────┬─────┘
                                          │
                                     ┌────▼─────┐
                                     │ data.db  │ ← single file
                                     └──────────┘
```

### Internal Architecture — The Stack

```
┌──────────────────────────────────────────────────────────────────┐
│           SQLite Internal Architecture                            │
│                                                                   │
│  SQL: SELECT * FROM users WHERE age > 25 ORDER BY name           │
│                                                                   │
│  ┌──────────────┐                                                │
│  │  Tokenizer    │  "SELECT" "FROM" "users" "WHERE" "age" ...    │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  Parser       │  Tokens → AST (parse tree)                    │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │ Code Generator│  AST → bytecode program for SQLite's VM       │
│  │ + Optimizer   │  Choose index, optimize joins, flatten subq.  │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  VDBE         │  Virtual DataBase Engine — register-based VM  │
│  │ (virtual      │  Executes bytecode: Open, SeekGE, Column,     │
│  │  machine)     │  ResultRow, Next, Halt                        │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  B-tree       │  Each table = B-tree (rowid → row data)       │
│  │  module       │  Each index = B-tree (indexed cols → rowid)   │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  Pager        │  Manages fixed-size pages (default 4KB)       │
│  │  (page cache) │  Handles caching, transactions, journaling    │
│  │              │  Ensures ACID (journal or WAL mode)            │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│  ┌──────────────┐                                                │
│  │  OS Interface │  Abstraction layer for file I/O + locks       │
│  │  (VFS)        │  Supports: Unix, Windows, custom (in-memory)  │
│  └──────┬───────┘                                                │
│         ▼                                                        │
│      data.db  (single file on disk)                              │
└──────────────────────────────────────────────────────────────────┘

You can see the bytecode with EXPLAIN:
  sqlite> EXPLAIN SELECT * FROM users WHERE age > 25;
  addr  opcode        p1    p2    p3
  ─────────────────────────────────
  0     Init          0     12    0
  1     OpenRead      0     2     0     (table users)
  2     OpenRead      1     3     0     (index on age)
  3     Integer       25    1     0
  4     SeekGT        1     10    1     (seek index: age > 25)
  5     IdxRowid      1     2     0     (get rowid from index)
  6     Seek          0     2     0     (seek table by rowid)
  7     Column        0     0     3     (read columns)
  8     ResultRow     3     3     0     (output row)
  9     Next          1     5     0     (next index entry)
  10    Close         1     0     0
  11    Close         0     0     0
  12    Halt          0     0     0

File format (single .db file):
  ┌──────────┬──────────┬──────────┬──────────┬─────┐
  │ Page 1   │ Page 2   │ Page 3   │ Page 4   │ ... │
  │ (header  │ (B-tree  │ (B-tree  │ (overflow │     │
  │  + root  │  interior│  leaf    │  page)    │     │
  │  B-tree) │  node)   │  node)   │           │     │
  └──────────┴──────────┴──────────┴──────────┴─────┘
  Page 1 always contains the file header + schema table.
  Everything is 4KB pages. Portable across OS/architecture.
```

## Key Concepts for Interviews

### 1. When SQLite Actually Scales
```
Litestream: replicate SQLite to S3 (disaster recovery)
LiteFS:     distributed SQLite at the edge (Fly.io)
Turso:      libSQL — SQLite fork with embedded replicas

Modern trend: run SQLite per-user or per-tenant at the edge.
  1000 users → 1000 SQLite databases
  Each user's data on the closest edge node
  No shared database bottleneck
```

### 2. WAL Mode (Write-Ahead Logging)
```sql
PRAGMA journal_mode=WAL;  -- enable WAL mode

Default mode: readers block writers
WAL mode:     readers and writers can work concurrently!

WAL = append changes to a log file → readers see snapshot
Same concept as PostgreSQL's WAL, just simpler.
```

### 3. Limitations That Matter
```
Concurrency:  1 writer at a time (readers are concurrent)
Network:      No client-server (file must be local)
Size:         Practical limit ~1TB (works but gets slow)
Auth:         No users/permissions (file permissions only)
Replication:  None built-in (use Litestream/LiteFS)
```

### 4. SQLite as a Learning Tool
SQLite implements the same concepts as PostgreSQL:
- B-tree indexes
- Query planner (EXPLAIN QUERY PLAN)
- Transactions (BEGIN, COMMIT, ROLLBACK)
- WAL for crash recovery
- VACUUM for space reclamation

Understanding SQLite = understanding 80% of how any SQL database works.

## SQLite vs Server Databases

| Aspect | SQLite | PostgreSQL |
|--------|--------|-----------|
| Deployment | Single file | Server process |
| Concurrency | 1 writer | Many writers |
| Network | Local only | Client-server |
| Setup | Zero config | Install + configure |
| Size | ~600KB | ~100MB |
| Best for | Embedded, edge | Shared, multi-user |

## Interview Sound Bite

> "For the mobile app's local data, I'd use SQLite — it gives us real SQL queries without running a server. For the edge caching layer, we could run SQLite-per-user with Turso for replication, so each user's data is on the nearest edge node with sub-millisecond reads."
