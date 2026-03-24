use rusqlite::Connection;

// =============================================================================
// Hierarchical Data — storing comment reply threads in SQL
//
//   Post: "What's the best database?"
//   ├── #1 (Alice): "PostgreSQL!"
//   │   ├── #3 (Bob): "Why not MySQL?"
//   │   │   └── #5 (Alice): "Better JSONB support"
//   │   └── #4 (Charlie): "+1 for Postgres"
//   └── #2 (Dave): "Depends on the use case"
//       └── #6 (Eve): "This. Always ask about access patterns first."
//
//   Option 1: Adjacency List (parent_id)
//     id │ parent_id │ author  │ content
//     ───┼───────────┼─────────┼─────────────────────
//      1 │ NULL      │ Alice   │ "PostgreSQL!"
//      3 │ 1         │ Bob     │ "Why not MySQL?"
//      5 │ 3         │ Alice   │ "Better JSONB support"
//     → Simple. Direct replies = WHERE parent_id = 1
//     → Full thread = WITH RECURSIVE
//
//   Option 2: Materialized Path
//     id │ path     │ author  │ content
//     ───┼──────────┼─────────┼─────────────────────
//      1 │ /1/      │ Alice   │ "PostgreSQL!"
//      3 │ /1/3/    │ Bob     │ "Why not MySQL?"
//      5 │ /1/3/5/  │ Alice   │ "Better JSONB support"
//     → All replies under #1: WHERE path LIKE '/1/%'
//     → ORDER BY path gives you threaded order for free
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Hierarchical Data (Comment Threads) ═══\n");

    let db = Connection::open_in_memory().unwrap();

    // ── Option 1: Adjacency List ──
    println!("    ── Option 1: Adjacency List (parent_id) ──\n");

    db.execute_batch("
        CREATE TABLE comments_adj (
            id INTEGER PRIMARY KEY,
            post_id INTEGER NOT NULL,
            parent_id INTEGER REFERENCES comments_adj(id),
            author TEXT NOT NULL,
            content TEXT NOT NULL,
            reply_count INTEGER DEFAULT 0,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX idx_comments_post ON comments_adj(post_id, parent_id);

        -- Post 10: 'What is the best database?'
        INSERT INTO comments_adj (id, post_id, parent_id, author, content) VALUES
            (1, 10, NULL, 'Alice',   'PostgreSQL!'),
            (2, 10, NULL, 'Dave',    'Depends on the use case'),
            (3, 10, 1,    'Bob',     'Why not MySQL?'),
            (4, 10, 1,    'Charlie', '+1 for Postgres'),
            (5, 10, 3,    'Alice',   'Better JSONB support'),
            (6, 10, 2,    'Eve',     'This. Always ask about access patterns first.');

        -- Pre-compute reply counts
        UPDATE comments_adj SET reply_count = (
            SELECT COUNT(*) FROM comments_adj c2 WHERE c2.parent_id = comments_adj.id
        );
    ").unwrap();

    // 1) Load top-level comments for a post
    println!("    Query: top-level comments for post 10");
    println!("    SQL: WHERE post_id = 10 AND parent_id IS NULL\n");
    let mut stmt: rusqlite::Statement<'_> = db.prepare(
        "SELECT id, author, content, reply_count FROM comments_adj
         WHERE post_id = 10 AND parent_id IS NULL ORDER BY id"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    #{} {}: \"{}\" ({} replies)",
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?, row.get::<_, i64>(3)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // 2) Click "show replies" on comment #1
    println!("\n    Query: replies to comment #1 (click 'show replies')");
    println!("    SQL: WHERE parent_id = 1\n");
    let mut stmt = db.prepare(
        "SELECT id, author, content, reply_count FROM comments_adj
         WHERE parent_id = 1 ORDER BY id"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("      └── #{} {}: \"{}\" ({} replies)",
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?, row.get::<_, i64>(3)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // 3) Full thread with recursive CTE
    println!("\n    Query: full threaded view (WITH RECURSIVE)\n");
    let mut stmt = db.prepare("
        WITH RECURSIVE thread AS (
            SELECT id, author, content, parent_id, 0 AS depth
            FROM comments_adj WHERE post_id = 10 AND parent_id IS NULL
            UNION ALL
            SELECT c.id, c.author, c.content, c.parent_id, t.depth + 1
            FROM comments_adj c
            JOIN thread t ON c.parent_id = t.id
        )
        SELECT id, author, content, depth FROM thread ORDER BY id
    ").unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        let depth: i64 = row.get(3)?;
        let indent = "  ".repeat(depth as usize);
        let prefix = if depth > 0 { "└── " } else { "" };
        Ok(format!("    {}{}#{} {}: \"{}\"",
            indent, prefix,
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // 4) Walk up parent chain — "show context" for a deep reply
    println!("\n    Query: ancestor chain for reply #5 ('show context')");
    println!("    SQL: WITH RECURSIVE ... walk up parent_id\n");
    let mut stmt = db.prepare("
        WITH RECURSIVE ancestors(id, author, content, parent_id, depth) AS (
            SELECT id, author, content, parent_id, 0 FROM comments_adj WHERE id = 5
            UNION ALL
            SELECT c.id, c.author, c.content, c.parent_id, a.depth + 1
            FROM comments_adj c
            JOIN ancestors a ON c.id = a.parent_id
        )
        SELECT id, author, content FROM ancestors ORDER BY depth DESC
    ").unwrap();
    let path: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("#{} {}: \"{}\"",
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
            row.get::<_, String>(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    println!("    {}\n", path.join(" → "));

    // ── Option 2: Materialized Path ──
    println!("    ── Option 2: Materialized Path ──\n");

    db.execute_batch("
        CREATE TABLE comments_path (
            id INTEGER PRIMARY KEY,
            post_id INTEGER NOT NULL,
            path TEXT NOT NULL,
            author TEXT NOT NULL,
            content TEXT NOT NULL
        );
        INSERT INTO comments_path VALUES (1, 10, '/1/',      'Alice',   'PostgreSQL!');
        INSERT INTO comments_path VALUES (2, 10, '/2/',      'Dave',    'Depends on the use case');
        INSERT INTO comments_path VALUES (3, 10, '/1/3/',    'Bob',     'Why not MySQL?');
        INSERT INTO comments_path VALUES (4, 10, '/1/4/',    'Charlie', '+1 for Postgres');
        INSERT INTO comments_path VALUES (5, 10, '/1/3/5/',  'Alice',   'Better JSONB support');
        INSERT INTO comments_path VALUES (6, 10, '/2/6/',    'Eve',     'This. Always ask about access patterns first.');
    ").unwrap();

    // All replies under comment #1 — just a LIKE prefix query, no recursion
    println!("    All replies under comment #1:");
    println!("    SQL: WHERE path LIKE '/1/%' ORDER BY path\n");
    let mut stmt = db.prepare(
        "SELECT id, path, author, content,
                LENGTH(path) - LENGTH(REPLACE(path, '/', '')) - 1 AS depth
         FROM comments_path WHERE path LIKE '/1/%' ORDER BY path"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        let depth: i64 = row.get(4)?;
        let indent = "  ".repeat(depth as usize);
        let prefix = if depth > 1 { "└── " } else { "" };
        Ok(format!("    {}{}#{} {}: \"{}\"  (path: {})",
            indent, prefix,
            row.get::<_, i64>(0)?, row.get::<_, String>(2)?,
            row.get::<_, String>(3)?, row.get::<_, String>(1)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    println!("\n    Adjacency list: simple, lazy-load replies (WHERE parent_id = ?).");
    println!("    Materialized path: fast full thread (ORDER BY path = threaded order).");
    println!("    Most apps: adjacency list + reply_count + limit depth to 2-3 levels.\n");
}
