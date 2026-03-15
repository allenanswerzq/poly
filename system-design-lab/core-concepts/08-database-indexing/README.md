# Database Indexing

## Why Indexes?

Without indexes, database queries require scanning every row:

```
SELECT * FROM users WHERE email = 'alice@example.com'

Without index: O(N) - scan all rows
With index:    O(log N) - B-tree lookup
```

## B-Tree Index (Most Common)

```
┌─────────────────────────────────────────────────────────────────┐
│                        B-Tree Structure                          │
│                                                                  │
│                         ┌───────────┐                           │
│                         │  [30,50]  │  Root                     │
│                         └─────┬─────┘                           │
│               ┌───────────────┼───────────────┐                 │
│               ▼               ▼               ▼                 │
│          ┌────────┐      ┌────────┐      ┌────────┐            │
│          │[10,20] │      │[35,40] │      │[60,70] │            │
│          └───┬────┘      └───┬────┘      └───┬────┘            │
│         ┌────┼────┐     ┌────┼────┐     ┌────┼────┐            │
│         ▼    ▼    ▼     ▼    ▼    ▼     ▼    ▼    ▼            │
│ Leaf:  [...][...][...] [...][...][...] [...][...][...]         │
│                                                                  │
│ Properties:                                                      │
│ • Balanced: all leaves at same depth                            │
│ • Sorted: keys in order within nodes                            │
│ • Range efficient: leaves linked (B+ tree)                      │
└─────────────────────────────────────────────────────────────────┘
```

## Index Operations Complexity

| Operation | Without Index | With B-Tree Index |
|-----------|---------------|-------------------|
| Point lookup (=) | O(N) | O(log N) |
| Range query | O(N) | O(log N + k) |
| Insert | O(1) | O(log N) |
| Delete | O(N) | O(log N) |

Note: k = number of matching rows in range

## Index Types

### 1. B-Tree/B+ Tree (Default)
```sql
CREATE INDEX idx_email ON users(email);

-- Good for:
WHERE email = 'x'           -- equality
WHERE age > 25              -- range
WHERE name LIKE 'Alice%'    -- prefix match
ORDER BY created_at         -- sorting
```

### 2. Hash Index
```sql
CREATE INDEX idx_hash ON users USING HASH (email);

-- Good for: equality only
WHERE email = 'x'           -- ✓
WHERE email > 'x'           -- ✗ (no range support)
```

### 3. Composite/Compound Index
```sql
CREATE INDEX idx_composite ON orders(user_id, created_at);

-- Uses index:
WHERE user_id = 1                       -- ✓ (leftmost)
WHERE user_id = 1 AND created_at > X    -- ✓ (both columns)
ORDER BY user_id, created_at            -- ✓

-- Does NOT use index:
WHERE created_at > X                    -- ✗ (not leftmost)
ORDER BY created_at, user_id            -- ✗ (wrong order)
```

### 4. Covering Index
```sql
CREATE INDEX idx_covering ON orders(user_id, status, total);

-- Index-only scan (no table access):
SELECT status, total FROM orders WHERE user_id = 1
```

## When to Index

### Good Candidates
- Primary keys (automatic)
- Foreign keys
- Columns in WHERE clauses
- Columns in JOIN conditions
- Columns used in ORDER BY

### Avoid Indexing
- Small tables (< 1000 rows)
- Low cardinality columns (gender: M/F)
- Frequently updated columns
- Columns rarely queried

## Index Overhead

```
INSERT/UPDATE/DELETE → Must update all indexes

Table with 5 indexes:
  INSERT one row → 6 write operations (1 table + 5 indexes)

Write-heavy workloads:
  Consider fewer indexes
  Consider partial indexes
```

## Query Planning

```sql
EXPLAIN SELECT * FROM users WHERE email = 'alice@example.com';

-- Without index:
Seq Scan on users  (rows=100000)
  Filter: (email = 'alice@example.com')

-- With index:
Index Scan using idx_email on users  (rows=1)
  Index Cond: (email = 'alice@example.com')
```

## Implementation

Our implementation demonstrates:
1. B-Tree insert and search
2. Range queries
3. Performance comparison (index vs scan)

Run the demo:
```bash
cargo run --bin database-indexing
```
