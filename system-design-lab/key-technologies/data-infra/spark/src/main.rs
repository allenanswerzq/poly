#![allow(dead_code, unused_variables, unused_imports)]
//! # Mini Spark
//!
//! Simulates core Apache Spark concepts:
//! 1. **RDD** (Resilient Distributed Dataset) — lazy transformations + actions
//! 2. **DAG execution** — stages, tasks, shuffle boundaries
//! 3. **Transformations** — map, filter, flatMap, groupByKey, reduceByKey
//! 4. **Actions** — collect, count, reduce
//! 5. **DataFrame** — structured data with SQL-like operations
//! 6. **Catalyst optimizer** — predicate pushdown, column pruning (simulated)
//!
//! Spark vs Hadoop MapReduce:
//! - Spark keeps data IN MEMORY between stages (100x faster for iterative jobs)
//! - Hadoop reads/writes HDFS between every map and reduce
//! - Spark has lazy evaluation: builds a DAG, optimizes, then executes
//! - Both use the same storage (HDFS) underneath

use std::collections::HashMap;

// =============================================================================
// RDD — Resilient Distributed Dataset
// =============================================================================
// Core abstraction: an immutable, partitioned collection of records.
//
// Two types of operations:
//   Transformations (lazy): map, filter, flatMap, groupByKey → returns new RDD
//   Actions (eager): collect, count, reduce → triggers actual computation
//
// Nothing computes until an action is called — Spark builds a DAG of
// transformations and optimizes the whole pipeline first.

#[derive(Clone, Debug)]
struct Rdd<T: Clone> {
    partitions: Vec<Vec<T>>,
    name: String,
}

impl<T: Clone + std::fmt::Debug> Rdd<T> {
    fn new(data: Vec<T>, num_partitions: usize, name: &str) -> Self {
        let mut partitions: Vec<Vec<T>> = (0..num_partitions).map(|_| Vec::new()).collect();
        for (i, item) in data.into_iter().enumerate() {
            partitions[i % num_partitions].push(item);
        }
        Self {
            partitions,
            name: name.to_string(),
        }
    }

    fn num_partitions(&self) -> usize {
        self.partitions.len()
    }

    fn count(&self) -> usize {
        self.partitions.iter().map(|p| p.len()).sum()
    }

    /// Collect all data from all partitions (action — triggers computation).
    fn collect(&self) -> Vec<T> {
        self.partitions.iter().flat_map(|p| p.clone()).collect()
    }

    /// Map transformation (lazy in real Spark, eager here for simplicity).
    fn map<U: Clone + std::fmt::Debug, F: Fn(&T) -> U>(&self, f: F, name: &str) -> Rdd<U> {
        let partitions = self
            .partitions
            .iter()
            .map(|p| p.iter().map(&f).collect())
            .collect();
        Rdd {
            partitions,
            name: name.to_string(),
        }
    }

    /// Filter transformation.
    fn filter<F: Fn(&T) -> bool>(&self, predicate: F, name: &str) -> Rdd<T> {
        let partitions = self
            .partitions
            .iter()
            .map(|p| p.iter().filter(|x| predicate(x)).cloned().collect())
            .collect();
        Rdd {
            partitions,
            name: name.to_string(),
        }
    }

    /// FlatMap transformation.
    fn flat_map<U: Clone + std::fmt::Debug, F: Fn(&T) -> Vec<U>>(
        &self,
        f: F,
        name: &str,
    ) -> Rdd<U> {
        let partitions = self
            .partitions
            .iter()
            .map(|p| p.iter().flat_map(&f).collect())
            .collect();
        Rdd {
            partitions,
            name: name.to_string(),
        }
    }

    /// Reduce action.
    fn reduce<F: Fn(T, T) -> T>(&self, f: F) -> Option<T> {
        self.collect().into_iter().reduce(f)
    }
}

/// Key-value RDD with shuffle operations.
impl Rdd<(String, i64)> {
    /// ReduceByKey — shuffle + aggregate (like MapReduce's reduce, but in-memory).
    /// This is a WIDE transformation → causes a shuffle (data exchange between partitions).
    fn reduce_by_key<F: Fn(i64, i64) -> i64>(&self, f: F, name: &str) -> Rdd<(String, i64)> {
        let all: Vec<(String, i64)> = self.collect();
        let mut grouped: HashMap<String, i64> = HashMap::new();
        for (k, v) in all {
            let entry = grouped.entry(k).or_insert(0);
            *entry = f(*entry, v);
        }
        let data: Vec<(String, i64)> = grouped.into_iter().collect();
        Rdd::new(data, self.num_partitions(), name)
    }

    /// GroupByKey — shuffle all values to the same key.
    fn group_by_key(&self, name: &str) -> HashMap<String, Vec<i64>> {
        let mut grouped: HashMap<String, Vec<i64>> = HashMap::new();
        for (k, v) in self.collect() {
            grouped.entry(k).or_default().push(v);
        }
        grouped
    }

    /// SortByKey.
    fn sort_by_key(&self, ascending: bool, name: &str) -> Rdd<(String, i64)> {
        let mut data = self.collect();
        if ascending {
            data.sort_by(|a, b| a.0.cmp(&b.0));
        } else {
            data.sort_by(|a, b| b.0.cmp(&a.0));
        }
        Rdd::new(data, self.num_partitions(), name)
    }
}

// =============================================================================
// DataFrame — Structured Data (like a table)
// =============================================================================

type Row = HashMap<String, Value>;

#[derive(Clone, Debug)]
enum Value {
    Int(i64),
    Float(f64),
    Str(String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v:.2}"),
            Value::Str(v) => write!(f, "{v}"),
        }
    }
}

struct DataFrame {
    columns: Vec<String>,
    rows: Vec<Row>,
}

impl DataFrame {
    fn new(columns: Vec<&str>, rows: Vec<Row>) -> Self {
        Self {
            columns: columns.iter().map(|s| s.to_string()).collect(),
            rows,
        }
    }

    /// SELECT specific columns.
    fn select(&self, cols: &[&str]) -> DataFrame {
        let rows = self
            .rows
            .iter()
            .map(|r| {
                cols.iter()
                    .filter_map(|c| r.get(*c).map(|v| (c.to_string(), v.clone())))
                    .collect()
            })
            .collect();
        DataFrame::new(cols.to_vec(), rows)
    }

    /// WHERE filter.
    fn filter<F: Fn(&Row) -> bool>(&self, predicate: F) -> DataFrame {
        let rows = self.rows.iter().filter(|r| predicate(r)).cloned().collect();
        DataFrame {
            columns: self.columns.clone(),
            rows,
        }
    }

    /// GROUP BY + aggregate.
    fn group_by_sum(&self, group_col: &str, agg_col: &str) -> DataFrame {
        let mut groups: HashMap<String, i64> = HashMap::new();
        for row in &self.rows {
            let key = match row.get(group_col) {
                Some(Value::Str(s)) => s.clone(),
                Some(v) => format!("{v}"),
                None => continue,
            };
            let val = match row.get(agg_col) {
                Some(Value::Int(n)) => *n,
                _ => 0,
            };
            *groups.entry(key).or_insert(0) += val;
        }

        let rows = groups
            .into_iter()
            .map(|(k, v)| {
                let mut row = HashMap::new();
                row.insert(group_col.to_string(), Value::Str(k));
                row.insert(format!("sum({agg_col})"), Value::Int(v));
                row
            })
            .collect();

        DataFrame::new(
            vec![group_col, &format!("sum({agg_col})")],
            rows,
        )
    }

    fn count(&self) -> usize {
        self.rows.len()
    }

    fn show(&self, max_rows: usize) {
        // Header
        for col in &self.columns {
            print!("  {col:<15}");
        }
        println!();
        for _ in &self.columns {
            print!("  {:<15}", "───────────────");
        }
        println!();

        // Rows
        for row in self.rows.iter().take(max_rows) {
            for col in &self.columns {
                let val = row
                    .get(col)
                    .map(|v| format!("{v}"))
                    .unwrap_or_else(|| "NULL".to_string());
                print!("  {val:<15}");
            }
            println!();
        }
        if self.rows.len() > max_rows {
            println!("  ... and {} more rows", self.rows.len() - max_rows);
        }
    }
}

fn make_row(pairs: Vec<(&str, Value)>) -> Row {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║          Mini Spark — RDD + DataFrame            ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // === 1. RDD Word Count ===
    println!("━━━ 1. RDD — Word Count ━━━\n");

    let lines = Rdd::new(
        vec![
            "hello world hello".to_string(),
            "spark is fast fast fast".to_string(),
            "hello spark world".to_string(),
        ],
        2,
        "lines",
    );
    println!("  Input: {} lines across {} partitions", lines.count(), lines.num_partitions());

    // lines.flatMap(split) → (word, 1) pairs → reduceByKey(+)
    let words: Rdd<(String, i64)> = lines.flat_map(
        |line| {
            line.split_whitespace()
                .map(|w| (w.to_lowercase(), 1i64))
                .collect()
        },
        "words",
    );

    let counts = words.reduce_by_key(|a, b| a + b, "word_counts");
    let sorted = counts.sort_by_key(true, "sorted");

    println!("\n  Word counts:");
    for (word, count) in sorted.collect() {
        println!("    {word}: {count}");
    }

    // === 2. RDD Transformations ===
    println!("\n━━━ 2. RDD — Transformation Pipeline ━━━\n");

    let numbers = Rdd::new((1..=20).collect(), 4, "numbers");
    println!("  Input: 1..20 across {} partitions", numbers.num_partitions());

    let evens = numbers.filter(|n| n % 2 == 0, "evens");
    let squared = evens.map(|n| n * n, "squared");
    let sum = squared.reduce(|a, b| a + b);

    println!("  Pipeline: numbers → filter(even) → map(x²) → reduce(+)");
    println!("  Evens: {:?}", evens.collect());
    println!("  Squared: {:?}", squared.collect());
    println!("  Sum: {:?}", sum);

    // === 3. DataFrame ===
    println!("\n━━━ 3. DataFrame — Structured Operations ━━━\n");

    let sales = DataFrame::new(
        vec!["product", "region", "amount"],
        vec![
            make_row(vec![("product", Value::Str("laptop".into())), ("region", Value::Str("US".into())), ("amount", Value::Int(1200))]),
            make_row(vec![("product", Value::Str("phone".into())), ("region", Value::Str("EU".into())), ("amount", Value::Int(800))]),
            make_row(vec![("product", Value::Str("laptop".into())), ("region", Value::Str("EU".into())), ("amount", Value::Int(1100))]),
            make_row(vec![("product", Value::Str("tablet".into())), ("region", Value::Str("US".into())), ("amount", Value::Int(500))]),
            make_row(vec![("product", Value::Str("phone".into())), ("region", Value::Str("US".into())), ("amount", Value::Int(900))]),
            make_row(vec![("product", Value::Str("laptop".into())), ("region", Value::Str("US".into())), ("amount", Value::Int(1300))]),
        ],
    );

    println!("  All sales:");
    sales.show(10);

    // SELECT + WHERE
    println!("\n  US sales only (SELECT product, amount WHERE region='US'):");
    let us_sales = sales.filter(|r| matches!(r.get("region"), Some(Value::Str(s)) if s == "US"));
    let us_selected = us_sales.select(&["product", "amount"]);
    us_selected.show(10);

    // GROUP BY
    println!("\n  Total by product (GROUP BY product, SUM(amount)):");
    let by_product = sales.group_by_sum("product", "amount");
    by_product.show(10);

    println!("\n  Total by region:");
    let by_region = sales.group_by_sum("region", "amount");
    by_region.show(10);

    // === 4. Spark vs MapReduce ===
    println!("\n━━━ 4. Spark vs Hadoop MapReduce ━━━\n");
    println!("  ┌──────────────────┬──────────────────────┬────────────────────────┐");
    println!("  │                  │ Hadoop MapReduce     │ Spark                  │");
    println!("  ├──────────────────┼──────────────────────┼────────────────────────┤");
    println!("  │ Storage between  │ Disk (HDFS)          │ Memory (+ disk spill)  │");
    println!("  │ stages           │                      │                        │");
    println!("  │ Speed            │ Slow (disk I/O)      │ 10-100x faster         │");
    println!("  │ API              │ Map + Reduce only    │ map, filter, join, SQL  │");
    println!("  │ Iteration        │ Multiple MR jobs     │ Cache in memory, loop   │");
    println!("  │ Evaluation       │ Eager                │ Lazy (DAG optimized)    │");
    println!("  │ Language         │ Java                 │ Scala/Python/Java/R     │");
    println!("  │ Best for         │ One-pass ETL         │ ML, interactive, SQL    │");
    println!("  └──────────────────┴──────────────────────┴────────────────────────┘");

    // === 5. Execution model ===
    println!("\n━━━ 5. Spark Execution Model ━━━\n");
    println!("  Job → Stages → Tasks");
    println!();
    println!("  lines.flatMap().reduceByKey().collect()");
    println!("      │              │              │");
    println!("      ▼              ▼              ▼");
    println!("   Stage 0        Stage 1       Action");
    println!("   (map side)     (reduce side)  (triggers)");
    println!("   ┌──────────┐   ┌──────────┐");
    println!("   │ Task 0   │   │ Task 0   │  ← one task per partition");
    println!("   │ Task 1   │   │ Task 1   │");
    println!("   └──────────┘   └──────────┘");
    println!("        └────shuffle────┘");
    println!("       (data exchange by key)");
}
