#![allow(dead_code, unused_variables, unused_imports)]
//! # Mini Hive
//!
//! Simulates core Apache Hive concepts:
//! 1. **SQL-on-Hadoop** — write SQL, Hive translates to MapReduce/Spark jobs
//! 2. **Metastore** — central catalog of table schemas, partitions, locations
//! 3. **Partitioning** — organize data by column (e.g., date) for fast queries
//! 4. **Bucketing** — hash-distribute within partitions for efficient joins
//! 5. **SerDe** — serializer/deserializer for different file formats
//!
//! Hive is NOT a database — it's a SQL interface over files in HDFS.
//! Think of it as: SQL → execution plan → MapReduce/Tez/Spark → read HDFS files

use std::collections::HashMap;

// =============================================================================
// Metastore — Central Schema Registry
// =============================================================================
// Stores: table name, columns, types, partition keys, file location, file format
// In production: backed by MySQL/PostgreSQL, shared across Hive/Spark/Presto

#[derive(Debug, Clone)]
enum ColumnType {
    Int,
    Float,
    String,
    Timestamp,
}

impl std::fmt::Display for ColumnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnType::Int => write!(f, "INT"),
            ColumnType::Float => write!(f, "FLOAT"),
            ColumnType::String => write!(f, "STRING"),
            ColumnType::Timestamp => write!(f, "TIMESTAMP"),
        }
    }
}

#[derive(Debug, Clone)]
struct Column {
    name: String,
    col_type: ColumnType,
}

#[derive(Debug, Clone)]
enum FileFormat {
    Csv,
    Parquet,
    Orc,
    Json,
}

impl std::fmt::Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileFormat::Csv => write!(f, "CSV"),
            FileFormat::Parquet => write!(f, "PARQUET"),
            FileFormat::Orc => write!(f, "ORC"),
            FileFormat::Json => write!(f, "JSON"),
        }
    }
}

#[derive(Debug, Clone)]
struct TableMetadata {
    name: String,
    database: String,
    columns: Vec<Column>,
    partition_keys: Vec<String>,
    format: FileFormat,
    location: String, // HDFS path
    num_rows: u64,
}

struct Metastore {
    tables: HashMap<String, TableMetadata>,
    partitions: HashMap<String, Vec<PartitionInfo>>, // table → partitions
}

#[derive(Debug, Clone)]
struct PartitionInfo {
    values: HashMap<String, String>, // partition_key → value
    location: String,
    num_rows: u64,
}

impl Metastore {
    fn new() -> Self {
        Self {
            tables: HashMap::new(),
            partitions: HashMap::new(),
        }
    }

    fn create_table(&mut self, table: TableMetadata) {
        let name = format!("{}.{}", table.database, table.name);
        println!("  CREATE TABLE {name}");
        println!("    columns: {}", table.columns.iter().map(|c| format!("{} {}", c.name, c.col_type)).collect::<Vec<_>>().join(", "));
        if !table.partition_keys.is_empty() {
            println!("    PARTITIONED BY ({})", table.partition_keys.join(", "));
        }
        println!("    STORED AS {}", table.format);
        println!("    LOCATION '{}'", table.location);
        self.tables.insert(name, table);
    }

    fn add_partition(&mut self, table_key: &str, partition: PartitionInfo) {
        self.partitions
            .entry(table_key.to_string())
            .or_default()
            .push(partition);
    }

    fn get_table(&self, key: &str) -> Option<&TableMetadata> {
        self.tables.get(key)
    }

    fn get_partitions(&self, table_key: &str) -> &[PartitionInfo] {
        self.partitions
            .get(table_key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn describe_table(&self, key: &str) {
        if let Some(t) = self.tables.get(key) {
            println!("  Table: {}.{}", t.database, t.name);
            println!("  Format: {}", t.format);
            println!("  Location: {}", t.location);
            println!("  Columns:");
            for col in &t.columns {
                println!("    {:<20} {}", col.name, col.col_type);
            }
            if !t.partition_keys.is_empty() {
                println!("  Partition Keys: {:?}", t.partition_keys);
                let parts = self.get_partitions(key);
                println!("  Partitions: {}", parts.len());
                for p in parts.iter().take(5) {
                    let vals: Vec<String> = p.values.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    println!("    {} ({} rows)", vals.join("/"), p.num_rows);
                }
            }
        }
    }
}

// =============================================================================
// SQL Parser + Query Planner (simplified)
// =============================================================================

#[derive(Debug)]
enum QueryPlan {
    TableScan {
        table: String,
        columns: Vec<String>,
    },
    Filter {
        input: Box<QueryPlan>,
        predicate: String,
    },
    Aggregate {
        input: Box<QueryPlan>,
        group_by: Vec<String>,
        aggregates: Vec<String>,
    },
    Join {
        left: Box<QueryPlan>,
        right: Box<QueryPlan>,
        on: String,
        join_type: String,
    },
}

impl QueryPlan {
    fn display(&self, indent: usize) {
        let pad = "  ".repeat(indent);
        match self {
            QueryPlan::TableScan { table, columns } => {
                println!("{pad}TableScan: {table} [{columns}]", columns = columns.join(", "));
            }
            QueryPlan::Filter { input, predicate } => {
                println!("{pad}Filter: {predicate}");
                input.display(indent + 1);
            }
            QueryPlan::Aggregate {
                input,
                group_by,
                aggregates,
            } => {
                println!(
                    "{pad}Aggregate: GROUP BY [{}], aggs=[{}]",
                    group_by.join(", "),
                    aggregates.join(", ")
                );
                input.display(indent + 1);
            }
            QueryPlan::Join {
                left,
                right,
                on,
                join_type,
            } => {
                println!("{pad}{join_type} JOIN ON {on}");
                left.display(indent + 1);
                right.display(indent + 1);
            }
        }
    }
}

fn plan_query(sql: &str) -> QueryPlan {
    // Very simplified — just demonstrates the concept
    if sql.contains("JOIN") {
        QueryPlan::Join {
            left: Box::new(QueryPlan::TableScan {
                table: "orders".to_string(),
                columns: vec!["order_id".into(), "user_id".into(), "amount".into()],
            }),
            right: Box::new(QueryPlan::TableScan {
                table: "users".to_string(),
                columns: vec!["user_id".into(), "name".into()],
            }),
            on: "orders.user_id = users.user_id".to_string(),
            join_type: "MAP".to_string(),
        }
    } else if sql.contains("GROUP BY") {
        QueryPlan::Aggregate {
            input: Box::new(QueryPlan::Filter {
                input: Box::new(QueryPlan::TableScan {
                    table: "events".to_string(),
                    columns: vec!["date".into(), "category".into(), "revenue".into()],
                }),
                predicate: "date >= '2024-01-01'".to_string(),
            }),
            group_by: vec!["category".to_string()],
            aggregates: vec!["SUM(revenue)".to_string(), "COUNT(*)".to_string()],
        }
    } else {
        QueryPlan::Filter {
            input: Box::new(QueryPlan::TableScan {
                table: "events".to_string(),
                columns: vec!["*".into()],
            }),
            predicate: "status = 'active'".to_string(),
        }
    }
}

// =============================================================================
// Partition Pruning Demo
// =============================================================================

fn partition_pruning_demo(metastore: &Metastore) {
    let key = "analytics.events";
    let all_parts = metastore.get_partitions(key);
    let total_rows: u64 = all_parts.iter().map(|p| p.num_rows).sum();

    // Query: WHERE date = '2024-03'
    let target_date = "2024-03";
    let pruned: Vec<&PartitionInfo> = all_parts
        .iter()
        .filter(|p| p.values.get("date").map(|v| v.as_str()) == Some(target_date))
        .collect();
    let pruned_rows: u64 = pruned.iter().map(|p| p.num_rows).sum();

    println!("  Query: SELECT * FROM events WHERE date = '{target_date}'");
    println!("  Without pruning: scan {total_rows} rows across {} partitions", all_parts.len());
    println!("  With pruning: scan {pruned_rows} rows across {} partition(s)", pruned.len());
    println!(
        "  Speedup: {:.0}x (skipped {:.0}% of data)",
        total_rows as f64 / pruned_rows.max(1) as f64,
        (1.0 - pruned_rows as f64 / total_rows as f64) * 100.0
    );
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      Mini Hive — SQL on Hadoop / Data Lake       ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // === 1. Metastore ===
    println!("━━━ 1. Hive Metastore — Schema Registry ━━━\n");

    let mut metastore = Metastore::new();

    metastore.create_table(TableMetadata {
        name: "events".to_string(),
        database: "analytics".to_string(),
        columns: vec![
            Column { name: "event_id".into(), col_type: ColumnType::Int },
            Column { name: "user_id".into(), col_type: ColumnType::Int },
            Column { name: "category".into(), col_type: ColumnType::String },
            Column { name: "revenue".into(), col_type: ColumnType::Float },
            Column { name: "ts".into(), col_type: ColumnType::Timestamp },
        ],
        partition_keys: vec!["date".to_string()],
        format: FileFormat::Parquet,
        location: "hdfs:///data/analytics/events".to_string(),
        num_rows: 0,
    });

    // Add partitions
    for month in ["2024-01", "2024-02", "2024-03", "2024-04", "2024-05", "2024-06"] {
        metastore.add_partition(
            "analytics.events",
            PartitionInfo {
                values: [("date".to_string(), month.to_string())].into_iter().collect(),
                location: format!("hdfs:///data/analytics/events/date={month}"),
                num_rows: 5_000_000,
            },
        );
    }

    println!();
    metastore.create_table(TableMetadata {
        name: "users".to_string(),
        database: "analytics".to_string(),
        columns: vec![
            Column { name: "user_id".into(), col_type: ColumnType::Int },
            Column { name: "name".into(), col_type: ColumnType::String },
            Column { name: "country".into(), col_type: ColumnType::String },
        ],
        partition_keys: vec![],
        format: FileFormat::Orc,
        location: "hdfs:///data/analytics/users".to_string(),
        num_rows: 10_000_000,
    });

    println!("\n━━━ 2. DESCRIBE TABLE ━━━\n");
    metastore.describe_table("analytics.events");

    // === 3. Query Planning ===
    println!("\n━━━ 3. Query Planning (SQL → Execution Plan) ━━━\n");

    let queries = [
        "SELECT category, SUM(revenue), COUNT(*) FROM events WHERE date >= '2024-01-01' GROUP BY category",
        "SELECT * FROM orders JOIN users ON orders.user_id = users.user_id",
    ];

    for sql in &queries {
        println!("  SQL: {sql}\n");
        println!("  Execution Plan:");
        let plan = plan_query(sql);
        plan.display(2);
        println!();
    }

    // === 4. Partition Pruning ===
    println!("━━━ 4. Partition Pruning ━━━\n");
    partition_pruning_demo(&metastore);

    // === 5. File Formats ===
    println!("\n━━━ 5. File Format Comparison ━━━\n");
    println!("  ┌──────────┬──────────┬───────────┬────────────────┬─────────────────────┐");
    println!("  │ Format   │ Type     │ Compress  │ Column Pruning │ Best For             │");
    println!("  ├──────────┼──────────┼───────────┼────────────────┼─────────────────────┤");
    println!("  │ CSV      │ Row      │ Poor      │ No             │ Import/export, debug │");
    println!("  │ JSON     │ Row      │ Poor      │ No             │ Semi-structured      │");
    println!("  │ Parquet  │ Columnar │ Excellent │ Yes            │ Analytics, Spark     │");
    println!("  │ ORC      │ Columnar │ Excellent │ Yes            │ Hive-optimized       │");
    println!("  │ Avro     │ Row      │ Good      │ No             │ Schema evolution     │");
    println!("  └──────────┴──────────┴───────────┴────────────────┴─────────────────────┘");
    println!();
    println!("  Columnar formats (Parquet/ORC) are 5-10x faster for analytics because");
    println!("  they skip entire columns you don't need in your SELECT.");

    // === 6. Hive Architecture ===
    println!("\n━━━ 6. Hive Architecture ━━━\n");
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │                      Hive Flow                              │");
    println!("  │                                                             │");
    println!("  │  SQL Query                                                  │");
    println!("  │    │                                                        │");
    println!("  │    ▼                                                        │");
    println!("  │  Parser → AST → Optimizer → Execution Plan                  │");
    println!("  │                                    │                        │");
    println!("  │                    ┌───────────────┼───────────────┐        │");
    println!("  │                    ▼               ▼               ▼        │");
    println!("  │              MapReduce          Tez           Spark         │");
    println!("  │                    │               │               │        │");
    println!("  │                    └───────────────┼───────────────┘        │");
    println!("  │                                    ▼                        │");
    println!("  │                              HDFS / S3                      │");
    println!("  │                         (Parquet / ORC files)               │");
    println!("  └─────────────────────────────────────────────────────────────┘");
}
