use rusqlite::Connection;

// =============================================================================
// Hierarchical Data — storing tree structures in flat SQL tables
//
//   Category tree:
//     Electronics
//     ├── Phones
//     │   ├── Apple
//     │   └── Samsung
//     └── Laptops
//         ├── Gaming
//         └── Business
//
//   Option 1: Adjacency List (parent_id)
//     id │ name        │ parent_id
//     ───┼─────────────┼──────────
//      1 │ Electronics │ NULL
//      2 │ Phones      │ 1
//      4 │ Apple       │ 2
//     → Simple, but finding ALL descendants needs WITH RECURSIVE
//
//   Option 2: Materialized Path
//     id │ name        │ path
//     ───┼─────────────┼──────────
//      1 │ Electronics │ /1/
//      2 │ Phones      │ /1/2/
//      4 │ Apple       │ /1/2/4/
//     → Fast subtree queries (WHERE path LIKE '/1/2/%')
//     → But moving a subtree = rewrite all descendants' paths
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Hierarchical Data (Tree Structures) ═══\n");

    let db = Connection::open_in_memory().unwrap();

    // ── Option 1: Adjacency List ──
    println!("    ── Option 1: Adjacency List (parent_id) ──\n");

    db.execute_batch("
        CREATE TABLE categories_adj (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            parent_id INTEGER REFERENCES categories_adj(id)
        );
        INSERT INTO categories_adj VALUES (1, 'Electronics', NULL);
        INSERT INTO categories_adj VALUES (2, 'Phones',      1);
        INSERT INTO categories_adj VALUES (3, 'Laptops',     1);
        INSERT INTO categories_adj VALUES (4, 'Apple',       2);
        INSERT INTO categories_adj VALUES (5, 'Samsung',     2);
        INSERT INTO categories_adj VALUES (6, 'Gaming',      3);
        INSERT INTO categories_adj VALUES (7, 'Business',    3);
    ").unwrap();

    // Direct children
    println!("    SQL: SELECT * FROM categories_adj WHERE parent_id = 1\n");
    let mut stmt = db.prepare(
        "SELECT name FROM categories_adj WHERE parent_id = 1"
    ).unwrap();
    let children: Vec<String> = stmt.query_map([], |r| r.get(0))
        .unwrap().filter_map(|r| r.ok()).collect();
    println!("    Direct children of Electronics: {}\n", children.join(", "));

    // ALL descendants using recursive CTE (WITH RECURSIVE)
    println!("    SQL: WITH RECURSIVE to find all descendants:\n");
    let mut stmt = db.prepare("
        WITH RECURSIVE descendants AS (
            SELECT id, name, parent_id, 1 AS depth
            FROM categories_adj WHERE parent_id = 1
            UNION ALL
            SELECT c.id, c.name, c.parent_id, d.depth + 1
            FROM categories_adj c
            JOIN descendants d ON c.parent_id = d.id
        )
        SELECT name, depth FROM descendants ORDER BY depth, name
    ").unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        let depth: i64 = row.get(1)?;
        let indent = "  ".repeat(depth as usize);
        Ok(format!("    {}→ {} (depth {})", indent, row.get::<_, String>(0)?, depth))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // Path from root to a leaf
    println!("\n    SQL: Walk up parent_id chain (path to 'Apple'):\n");
    let mut stmt = db.prepare("
        WITH RECURSIVE path(id, name, parent_id, depth) AS (
            SELECT id, name, parent_id, 0 FROM categories_adj WHERE name = 'Apple'
            UNION ALL
            SELECT c.id, c.name, c.parent_id, p.depth + 1
            FROM categories_adj c
            JOIN path p ON c.id = p.parent_id
        )
        SELECT name FROM path ORDER BY depth DESC
    ").unwrap();
    let path: Vec<String> = stmt.query_map([], |r| r.get(0))
        .unwrap().filter_map(|r| r.ok()).collect();
    println!("    Path: {}\n", path.join(" → "));

    // ── Option 2: Materialized Path ──
    println!("    ── Option 2: Materialized Path ──\n");

    db.execute_batch("
        CREATE TABLE categories_path (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL  -- e.g., '/1/2/4/'
        );
        INSERT INTO categories_path VALUES (1, 'Electronics', '/1/');
        INSERT INTO categories_path VALUES (2, 'Phones',      '/1/2/');
        INSERT INTO categories_path VALUES (3, 'Laptops',     '/1/3/');
        INSERT INTO categories_path VALUES (4, 'Apple',       '/1/2/4/');
        INSERT INTO categories_path VALUES (5, 'Samsung',     '/1/2/5/');
        INSERT INTO categories_path VALUES (6, 'Gaming',      '/1/3/6/');
        INSERT INTO categories_path VALUES (7, 'Business',    '/1/3/7/');
    ").unwrap();

    // Find all descendants with simple LIKE query — no recursion needed!
    println!("    SQL: SELECT * FROM categories_path WHERE path LIKE '/1/2/%'\n");
    let mut stmt = db.prepare(
        "SELECT name, path FROM categories_path WHERE path LIKE '/1/2/%' AND id != 2"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    → {} (path: {})", row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    println!("    Descendants of Phones:");
    for row in &rows { println!("{}", row); }

    // Get depth from path (count slashes - 1)
    println!("\n    All categories with depth:");
    let mut stmt = db.prepare(
        "SELECT name, path, LENGTH(path) - LENGTH(REPLACE(path, '/', '')) - 1 AS depth
         FROM categories_path ORDER BY path"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    {} (depth {}, path: {})",
            row.get::<_, String>(0)?, row.get::<_, i64>(2)?, row.get::<_, String>(1)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    println!("\n    Adjacency list: simple, but needs recursive CTE for subtrees.");
    println!("    Materialized path: fast subtree queries (LIKE prefix), but moving subtrees is expensive.\n");
}
