# MongoDB Deep Dive

## Overview

MongoDB is the **most popular document database**. Instead of rows and tables, it stores flexible JSON-like documents in collections. Choose it when your data schema varies per record or changes frequently.

## When to Choose MongoDB

| Use Case | Why MongoDB |
|----------|-----------|
| Product catalogs | Different products have different attributes |
| Content management | Articles, pages with varying structure |
| User profiles | Varying fields per user type |
| Prototyping | Schema can evolve without migrations |
| Embedded 1:N data | Reviews inside products, comments inside posts |

## Data Model

```
SQL table (rigid):                 MongoDB document (flexible):
┌────┬───────┬───────────┐        {
│ id │ name  │ email     │          "_id": ObjectId("..."),
├────┼───────┼───────────┤          "name": "Alice",
│  1 │ Alice │ a@b.com   │          "email": "a@b.com",
│  2 │ Bob   │ b@b.com   │          "addresses": [           ← embedded array
└────┴───────┴───────────┘            {"city": "NYC", "zip": "10001"},
                                      {"city": "LA", "zip": "90001"}
Every row must have                 ],
the same columns.                   "preferences": {          ← nested object
                                      "theme": "dark",
                                      "notifications": true
                                    }
                                  }

                                  Different docs can have
                                  completely different fields.
```

## Key Concepts for Interviews

### 1. Embedding vs Referencing
```
Embedding (denormalized):          Referencing (normalized):
┌──────────────────────┐          ┌──────────────┐  ┌──────────────┐
│ order: {             │          │ order: {     │  │ user: {      │
│   user_name: "Alice" │          │   user_id: 1 │──│   name: Alice│
│   user_email: "..."  │          │   items: [...]│  │   email: ... │
│   items: [...]       │          │ }            │  │ }            │
│ }                    │          └──────────────┘  └──────────────┘
└──────────────────────┘
Fast reads (1 query)              Consistent data (no duplication)
Duplicated data                    Requires $lookup (slow JOIN)
```

**Rule of thumb**: Embed data that's read together. Reference data that's updated independently.

### 2. Sharding (Horizontal Scaling)
```
Shard key: user_id

Shard 1 (user_id 1-1000)     Shard 2 (user_id 1001-2000)
┌─────────────────────┐     ┌─────────────────────────┐
│ Alice's docs        │     │ Charlie's docs          │
│ Bob's docs          │     │ Dave's docs             │
└─────────────────────┘     └─────────────────────────┘

Config servers (metadata)    Mongos routers (query routing)
```

### 3. Indexes
- B-tree indexes (default, same as SQL)
- Compound indexes: `{user_id: 1, created_at: -1}`
- Text indexes for full-text search
- Geospatial indexes (2dsphere)
- **If you don't index, every query is a collection scan** (same as SQL)

### 4. Aggregation Pipeline
```javascript
// SQL: SELECT status, COUNT(*) FROM orders GROUP BY status
db.orders.aggregate([
  { $group: { _id: "$status", count: { $sum: 1 } } },
  { $sort: { count: -1 } }
])
```

### 5. Replica Sets
```
Primary ──► Secondary 1 (auto-failover)
        ──► Secondary 2
        ──► Arbiter (votes only, no data)

- Automatic failover (election in ~10 seconds)
- Read from secondaries for read scaling
- Write concern: "majority" = wait for 2/3 nodes
```

## MongoDB vs PostgreSQL JSONB

| Aspect | MongoDB | PostgreSQL JSONB |
|--------|---------|-----------------|
| Schema flexibility | Native, first-class | Bolt-on (JSONB column) |
| JOINs | Weak ($lookup) | Native, optimized |
| Transactions | Multi-doc since v4.0 | ACID from day one |
| Scaling | Native sharding | Extensions (Citus) |
| Tooling | Atlas, Compass | pgAdmin, psql |
| Best for | Document-centric apps | Mixed relational + JSON |

## Limitations to Mention

- No true JOINs (only $lookup, expensive)
- Multi-document transactions have overhead (added in v4.0, not free)
- Denormalization means update anomalies
- Large documents (>16MB limit) need GridFS
- Schema validation is optional (discipline required)

## Interview Sound Bite

> "I'd use MongoDB for the product catalog because each product type has different attributes — a phone has 'screen_size' and 'battery', while a shirt has 'size' and 'color'. With MongoDB we embed these varying attributes directly without schema migrations. For the order system that needs ACID transactions, I'd pair it with PostgreSQL."
