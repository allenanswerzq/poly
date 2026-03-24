use rusqlite::Connection;
use std::time::Instant;

// =============================================================================
// Indexing — trade write speed for read speed
//
//   Without index (full table scan):
//     SELECT * FROM users WHERE email = 'alice@example.com'
//     → Check EVERY row: row 1? no. row 2? no. ... row 99999? no. row 100000? YES!
//     → O(n) — 100K comparisons
//
//   With B-tree index on email:
//     ┌────────────────────────┐
//     │      m                 │  ← root: is 'alice' < 'm'? go left
//     │     / \                │
//     │    d   r               │  ← 'alice' < 'd'? go left
//     │   / \   \              │
//     │  a   f   z             │  ← found 'a' bucket → alice is here
//     └────────────────────────┘
//     → O(log n) — ~17 comparisons for 100K rows
//
//   But every INSERT must also update the index:
//     INSERT → write row + update B-tree = slower writes
//
//   Rule: index columns you frequently query on (WHERE, JOIN, ORDER BY)
//         but don't over-index (each index slows writes)
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Indexing Effects ═══\n");

    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL;").unwrap();

    db.execute_batch("
        CREATE TABLE users_no_idx (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            age INTEGER NOT NULL
        );
        CREATE TABLE users_with_idx (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            age INTEGER NOT NULL
        );
        CREATE INDEX idx_email ON users_with_idx(email);
        CREATE INDEX idx_age ON users_with_idx(age);
    ").unwrap();

    // Insert 100K rows into both tables
    let n = 100_000;
    println!("    Inserting {} rows into two tables...\n", n);

    let start = Instant::now();
    db.execute_batch("BEGIN").unwrap();
    for i in 0..n {
        let age = 18 + (i % 62);
        db.execute(
            "INSERT INTO users_no_idx VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![i, format!("User{}", i), format!("user{}@example.com", i), age]
        ).unwrap();
    }
    db.execute_batch("COMMIT").unwrap();
    let no_idx_insert = start.elapsed();

    let start = Instant::now();
    db.execute_batch("BEGIN").unwrap();
    for i in 0..n {
        let age = 18 + (i % 62);
        db.execute(
            "INSERT INTO users_with_idx VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![i, format!("User{}", i), format!("user{}@example.com", i), age]
        ).unwrap();
    }
    db.execute_batch("COMMIT").unwrap();
    let with_idx_insert = start.elapsed();

    println!("    Insert {} rows WITHOUT indexes: {:?}", n, no_idx_insert);
    println!("    Insert {} rows WITH 2 indexes:  {:?}", n, with_idx_insert);
    println!("    Indexes slow writes by ~{:.0}%\n",
        (with_idx_insert.as_nanos() as f64 / no_idx_insert.as_nanos() as f64 - 1.0) * 100.0);

    // Point lookup by email
    let target = "user50000@example.com";
    println!("    Point lookup: WHERE email = '{}'\n", target);

    let start = Instant::now();
    let _: String = db.query_row(
        "SELECT name FROM users_no_idx WHERE email = ?1", [target], |r| r.get(0)
    ).unwrap();
    let scan_time = start.elapsed();

    let start = Instant::now();
    let _: String = db.query_row(
        "SELECT name FROM users_with_idx WHERE email = ?1", [target], |r| r.get(0)
    ).unwrap();
    let idx_time = start.elapsed();

    println!("    WITHOUT index (full table scan): {:?}", scan_time);
    println!("    WITH index (B-tree lookup):      {:?}", idx_time);
    if idx_time.as_nanos() > 0 {
        println!("    Speedup: ~{:.0}x faster\n",
            scan_time.as_nanos() as f64 / idx_time.as_nanos().max(1) as f64);
    }

    // Show EXPLAIN QUERY PLAN
    println!("    EXPLAIN QUERY PLAN (without index):");
    let mut stmt = db.prepare(
        "EXPLAIN QUERY PLAN SELECT name FROM users_no_idx WHERE email = 'user50000@example.com'"
    ).unwrap();
    let plans: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(3))
        .unwrap().filter_map(|r| r.ok()).collect();
    for p in &plans { println!("    → {}", p); }

    println!("\n    EXPLAIN QUERY PLAN (with index):");
    let mut stmt = db.prepare(
        "EXPLAIN QUERY PLAN SELECT name FROM users_with_idx WHERE email = 'user50000@example.com'"
    ).unwrap();
    let plans: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(3))
        .unwrap().filter_map(|r| r.ok()).collect();
    for p in &plans { println!("    → {}", p); }

    // Range query
    println!("\n    Range query: WHERE age BETWEEN 18 AND 25\n");

    let start = Instant::now();
    let count_no: i64 = db.query_row(
        "SELECT COUNT(*) FROM users_no_idx WHERE age BETWEEN 18 AND 25", [], |r| r.get(0)
    ).unwrap();
    let scan_time = start.elapsed();

    let start = Instant::now();
    let count_idx: i64 = db.query_row(
        "SELECT COUNT(*) FROM users_with_idx WHERE age BETWEEN 18 AND 25", [], |r| r.get(0)
    ).unwrap();
    let idx_time = start.elapsed();

    println!("    WITHOUT index: {} rows in {:?}", count_no, scan_time);
    println!("    WITH index:    {} rows in {:?}", count_idx, idx_time);

    // Summary
    println!("\n    ┌─────────────────┬──────────────┬──────────────┐");
    println!("    │                 │ Without Index │ With Index    │");
    println!("    ├─────────────────┼──────────────┼──────────────┤");
    println!("    │ Point lookup    │ O(n) SCAN    │ O(log n)      │");
    println!("    │ Range query     │ O(n) SCAN    │ O(log n + k)  │");
    println!("    │ INSERT speed    │ Faster       │ Slower        │");
    println!("    │ Storage         │ None extra   │ B-tree index  │");
    println!("    └─────────────────┴──────────────┴──────────────┘");
    println!("\n    Rule: index columns you query on, but don't over-index.\n");
}
