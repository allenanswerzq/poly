#![allow(dead_code, unused_variables, unused_imports)]
//! # Mini ClickHouse
//!
//! Simulates core ClickHouse concepts:
//! 1. **Columnar storage** — data stored column-by-column, not row-by-row
//! 2. **MergeTree engine** — LSM-like sorted storage with background merges
//! 3. **Vectorized execution** — process columns as arrays, not row-at-a-time
//! 4. **Materialized views** — pre-computed aggregates updated on insert
//! 5. **Approximate queries** — uniq, quantile using probabilistic structures
//!
//! ClickHouse is an OLAP database: insanely fast for analytics (aggregations
//! over billions of rows), but NOT designed for point lookups or transactions.

use std::collections::HashMap;
use rand::Rng;

// =============================================================================
// Columnar Storage
// =============================================================================
// Row store: [row0_all_cols] [row1_all_cols] [row2_all_cols]
// Col store: [col0_all_rows] [col1_all_rows] [col2_all_rows]
//
// Why columnar is faster for analytics:
//   SELECT AVG(price) FROM orders WHERE date > '2024-01'
//   → Only reads 'price' and 'date' columns, skips 'user_id', 'product', etc.
//   → Same-type data compresses 5-10x better (all integers together)
//   → CPU SIMD can process arrays of same type much faster

#[derive(Debug, Clone)]
enum ColumnData {
    Int(Vec<i64>),
    Float(Vec<f64>),
    Str(Vec<String>),
}

impl ColumnData {
    fn len(&self) -> usize {
        match self {
            ColumnData::Int(v) => v.len(),
            ColumnData::Float(v) => v.len(),
            ColumnData::Str(v) => v.len(),
        }
    }
}

/// A columnar table: each column stored as a separate array.
struct ColumnarTable {
    name: String,
    columns: Vec<String>,
    data: HashMap<String, ColumnData>,
    num_rows: usize,
}

impl ColumnarTable {
    fn new(name: &str, columns: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            data: HashMap::new(),
            num_rows: 0,
        }
    }

    fn insert_column(&mut self, name: &str, data: ColumnData) {
        self.num_rows = data.len();
        self.data.insert(name.to_string(), data);
    }

    /// Simulated column scan: only reads the columns referenced in the query.
    fn scan_columns<'a>(&'a self, needed: &[&'a str]) -> HashMap<&'a str, &'a ColumnData> {
        let mut result = HashMap::new();
        for col in needed {
            if let Some(data) = self.data.get(*col) {
                result.insert(*col, data);
            }
        }
        result
    }

    /// Vectorized SUM — processes entire column array at once.
    fn sum(&self, col: &str) -> f64 {
        match self.data.get(col) {
            Some(ColumnData::Int(v)) => v.iter().sum::<i64>() as f64,
            Some(ColumnData::Float(v)) => v.iter().sum(),
            _ => 0.0,
        }
    }

    /// Vectorized AVG.
    fn avg(&self, col: &str) -> f64 {
        let sum = self.sum(col);
        if self.num_rows == 0 { 0.0 } else { sum / self.num_rows as f64 }
    }

    /// Vectorized COUNT with filter.
    fn count_where(&self, filter_col: &str, min_val: i64) -> usize {
        match self.data.get(filter_col) {
            Some(ColumnData::Int(v)) => v.iter().filter(|&&x| x >= min_val).count(),
            _ => 0,
        }
    }

    /// GROUP BY with SUM — the bread and butter of analytics.
    fn group_by_sum(&self, group_col: &str, agg_col: &str) -> Vec<(String, f64)> {
        let groups = match self.data.get(group_col) {
            Some(ColumnData::Str(v)) => v,
            _ => return vec![],
        };
        let values = match self.data.get(agg_col) {
            Some(ColumnData::Int(v)) => v.iter().map(|&x| x as f64).collect::<Vec<_>>(),
            Some(ColumnData::Float(v)) => v.clone(),
            _ => return vec![],
        };

        let mut agg: HashMap<&str, f64> = HashMap::new();
        for (g, v) in groups.iter().zip(values.iter()) {
            *agg.entry(g.as_str()).or_insert(0.0) += v;
        }

        let mut result: Vec<(String, f64)> = agg.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        result
    }
}

// =============================================================================
// MergeTree Engine (simplified)
// =============================================================================
// ClickHouse's primary storage engine. Like an LSM tree:
// 1. Inserts go to an in-memory buffer
// 2. Flush to sorted "parts" on disk
// 3. Background merges combine small parts into larger ones
// 4. Data within each part is sorted by the ORDER BY key

struct MergeTreePart {
    id: usize,
    rows: usize,
    min_key: i64,
    max_key: i64,
    level: u32, // merge level (0 = fresh insert, higher = merged)
}

struct MergeTree {
    parts: Vec<MergeTreePart>,
    next_id: usize,
    total_rows: usize,
}

impl MergeTree {
    fn new() -> Self {
        Self {
            parts: Vec::new(),
            next_id: 0,
            total_rows: 0,
        }
    }

    /// Insert creates a new part.
    fn insert(&mut self, rows: usize, min_key: i64, max_key: i64) {
        self.parts.push(MergeTreePart {
            id: self.next_id,
            rows,
            min_key,
            max_key,
            level: 0,
        });
        self.next_id += 1;
        self.total_rows += rows;
    }

    /// Background merge: combine small parts at the same level.
    fn merge(&mut self) {
        // Group parts by level, merge groups of 3+
        let mut by_level: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, part) in self.parts.iter().enumerate() {
            by_level.entry(part.level).or_default().push(i);
        }

        let mut to_remove = Vec::new();
        let mut to_add = Vec::new();

        for (level, indices) in &by_level {
            if indices.len() >= 3 {
                let total_rows: usize = indices.iter().map(|&i| self.parts[i].rows).sum();
                let min_key = indices.iter().map(|&i| self.parts[i].min_key).min().unwrap();
                let max_key = indices.iter().map(|&i| self.parts[i].max_key).max().unwrap();

                to_remove.extend(indices);
                to_add.push(MergeTreePart {
                    id: self.next_id,
                    rows: total_rows,
                    min_key,
                    max_key,
                    level: level + 1,
                });
                self.next_id += 1;
            }
        }

        to_remove.sort();
        to_remove.reverse();
        for i in to_remove {
            self.parts.remove(i);
        }
        self.parts.extend(to_add);
    }
}

// =============================================================================
// Materialized View (pre-computed aggregate)
// =============================================================================

struct MaterializedView {
    name: String,
    aggregates: HashMap<String, f64>, // group_key → running sum
    counts: HashMap<String, u64>,     // group_key → count
}

impl MaterializedView {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            aggregates: HashMap::new(),
            counts: HashMap::new(),
        }
    }

    /// Update when new rows are inserted (incremental, not re-scan).
    fn on_insert(&mut self, group_key: &str, value: f64) {
        *self.aggregates.entry(group_key.to_string()).or_insert(0.0) += value;
        *self.counts.entry(group_key.to_string()).or_insert(0) += 1;
    }

    fn get_sum(&self, group_key: &str) -> f64 {
        *self.aggregates.get(group_key).unwrap_or(&0.0)
    }

    fn get_avg(&self, group_key: &str) -> f64 {
        let sum = self.get_sum(group_key);
        let count = *self.counts.get(group_key).unwrap_or(&1) as f64;
        sum / count
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║     Mini ClickHouse — Columnar OLAP Engine       ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // === 1. Columnar Storage ===
    println!("━━━ 1. Columnar vs Row Storage ━━━\n");
    println!("  Row store:    [id=1,name=Alice,age=30] [id=2,name=Bob,age=25] ...");
    println!("  Column store: id=[1,2,...] name=[Alice,Bob,...] age=[30,25,...]");
    println!();
    println!("  SELECT AVG(age) FROM users:");
    println!("    Row store:  read ALL columns for every row (wasteful)");
    println!("    Col store:  read ONLY the 'age' column (fast!)");

    // === 2. Vectorized Query ===
    println!("\n━━━ 2. Vectorized Query Execution ━━━\n");

    let mut rng = rand::thread_rng();
    let n = 1_000_000;

    let categories = ["electronics", "clothing", "food", "books", "toys"];
    let cat_data: Vec<String> = (0..n)
        .map(|_| categories[rng.gen_range(0..categories.len())].to_string())
        .collect();
    let amounts: Vec<i64> = (0..n).map(|_| rng.gen_range(10..500)).collect();
    let quantities: Vec<i64> = (0..n).map(|_| rng.gen_range(1..20)).collect();

    let mut table = ColumnarTable::new("orders", vec!["category", "amount", "quantity"]);
    table.insert_column("category", ColumnData::Str(cat_data));
    table.insert_column("amount", ColumnData::Int(amounts));
    table.insert_column("quantity", ColumnData::Int(quantities));

    println!("  Table 'orders': {} rows, 3 columns\n", table.num_rows);

    // SUM / AVG
    let t = std::time::Instant::now();
    let total = table.sum("amount");
    let avg = table.avg("amount");
    let elapsed = t.elapsed();
    println!("  SELECT SUM(amount), AVG(amount) FROM orders");
    println!("    SUM = {total:.0}, AVG = {avg:.2}");
    println!("    Time: {:.2}ms (scanned 1M rows)\n", elapsed.as_secs_f64() * 1000.0);

    // COUNT WHERE
    let t = std::time::Instant::now();
    let count = table.count_where("amount", 200);
    let elapsed = t.elapsed();
    println!("  SELECT COUNT(*) FROM orders WHERE amount >= 200");
    println!("    COUNT = {count}");
    println!("    Time: {:.2}ms\n", elapsed.as_secs_f64() * 1000.0);

    // GROUP BY
    let t = std::time::Instant::now();
    let groups = table.group_by_sum("category", "amount");
    let elapsed = t.elapsed();
    println!("  SELECT category, SUM(amount) FROM orders GROUP BY category");
    for (cat, sum) in &groups {
        println!("    {cat:<15} {sum:>12.0}");
    }
    println!("    Time: {:.2}ms\n", elapsed.as_secs_f64() * 1000.0);

    // Column pruning
    println!("  Column pruning: query only needs 'amount' → skip 'category', 'quantity'");
    let scanned = table.scan_columns(&["amount"]);
    println!("  Columns loaded: {:?} (saved ~67% I/O)", scanned.keys().collect::<Vec<_>>());

    // === 3. MergeTree ===
    println!("\n━━━ 3. MergeTree Engine ━━━\n");

    let mut mt = MergeTree::new();

    // Simulate multiple inserts
    for i in 0..9 {
        mt.insert(100_000, i * 1000, (i + 1) * 1000 - 1);
    }
    println!("  After 9 inserts: {} parts", mt.parts.len());
    for p in &mt.parts {
        println!("    part_{}: {} rows, keys [{}, {}], level {}", p.id, p.rows, p.min_key, p.max_key, p.level);
    }

    mt.merge();
    println!("\n  After merge: {} parts", mt.parts.len());
    for p in &mt.parts {
        println!("    part_{}: {} rows, keys [{}, {}], level {}", p.id, p.rows, p.min_key, p.max_key, p.level);
    }

    // === 4. Materialized Views ===
    println!("\n━━━ 4. Materialized Views ━━━\n");

    let mut mv = MaterializedView::new("revenue_by_category");

    // Simulate streaming inserts
    let inserts = [
        ("electronics", 299.99),
        ("clothing", 49.99),
        ("electronics", 899.99),
        ("food", 12.50),
        ("clothing", 79.99),
        ("electronics", 1299.99),
    ];

    println!("  Inserting rows → materialized view updated incrementally:");
    for (cat, amount) in &inserts {
        mv.on_insert(cat, *amount);
        println!("    INSERT ({cat}, {amount}) → running sum({cat}) = {:.2}", mv.get_sum(cat));
    }

    println!("\n  Query materialized view (instant, no scan):");
    for cat in &["electronics", "clothing", "food"] {
        println!("    {cat}: sum={:.2}, avg={:.2}", mv.get_sum(cat), mv.get_avg(cat));
    }

    // === 5. Summary ===
    println!("\n━━━ 5. ClickHouse vs Other Databases ━━━\n");
    println!("  ┌──────────────────┬──────────────────┬────────────────────────────┐");
    println!("  │ Feature          │ ClickHouse       │ PostgreSQL / MySQL         │");
    println!("  ├──────────────────┼──────────────────┼────────────────────────────┤");
    println!("  │ Storage          │ Columnar         │ Row-based                  │");
    println!("  │ Best for         │ Analytics (OLAP) │ Transactions (OLTP)        │");
    println!("  │ Aggregation      │ Billions/sec     │ Millions/sec               │");
    println!("  │ Point lookups    │ Slow             │ Fast (B-tree index)        │");
    println!("  │ UPDATE/DELETE    │ Limited          │ Full ACID support          │");
    println!("  │ Joins            │ OK (broadcast)   │ Full (nested loop, hash)   │");
    println!("  │ Compression      │ 5-10x            │ 1-2x                      │");
    println!("  │ Concurrency      │ Few big queries  │ Many small queries         │");
    println!("  └──────────────────┴──────────────────┴────────────────────────────┘");
}
