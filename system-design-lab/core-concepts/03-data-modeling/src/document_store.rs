use rusqlite::Connection;

// =============================================================================
// Document Store — flexible JSON documents (like MongoDB)
//
//   products collection:
//   ┌────────────────────────────────────────────┐
//   │ { "name": "Widget",                       │
//   │   "price": 9.99,                          │
//   │   "tags": ["electronics"],                │
//   │   "specs": {"weight": "200g"} }           │  ← document 1
//   ├────────────────────────────────────────────┤
//   │ { "name": "Gadget",                       │
//   │   "price": 24.99,                         │
//   │   "specs": {"battery": "lithium"} }       │  ← different fields!
//   ├────────────────────────────────────────────┤
//   │ { "name": "Doohickey",                    │
//   │   "reviews": [{"user":"Alice","rating":5}]│  ← embedded array
//   │ }                                          │
//   └────────────────────────────────────────────┘
//
//   No fixed schema — each document can have different fields
//   Related data embedded directly (reviews inside product)
//
//   Pros: flexible schema, nested data, no JOINs for related data
//   Cons: no referential integrity, data duplication
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Document Store (JSON in SQL) ═══\n");

    let db = Connection::open_in_memory().unwrap();

    db.execute_batch("
        CREATE TABLE products (
            id TEXT PRIMARY KEY,
            data JSON NOT NULL
        );
    ").unwrap();

    // Different documents can have DIFFERENT fields — flexible schema
    db.execute("INSERT INTO products VALUES ('prod-1', ?)", [r#"{
        "name": "Widget",
        "price": 9.99,
        "tags": ["electronics", "gadgets"],
        "specs": {"weight": "200g", "color": "blue"}
    }"#]).unwrap();

    db.execute("INSERT INTO products VALUES ('prod-2', ?)", [r#"{
        "name": "Gadget",
        "price": 24.99,
        "tags": ["electronics"],
        "specs": {"weight": "500g", "color": "red", "battery": "lithium"}
    }"#]).unwrap();

    // prod-3 has reviews embedded — different structure entirely
    db.execute("INSERT INTO products VALUES ('prod-3', ?)", [r#"{
        "name": "Doohickey",
        "price": 4.99,
        "reviews": [
            {"user": "Alice", "rating": 5, "text": "Love it!"},
            {"user": "Bob", "rating": 3, "text": "Its okay"}
        ]
    }"#]).unwrap();

    // Read embedded document — no JOINs
    println!("    Read product with embedded reviews (no JOIN):");
    println!("    SQL: SELECT data FROM products WHERE id = 'prod-3'\n");
    let doc: String = db.query_row(
        "SELECT json_pretty(data) FROM products WHERE id = 'prod-3'", [], |r| r.get(0)
    ).unwrap();
    for line in doc.lines() { println!("    {}", line); }

    // Query INTO JSON fields using json_extract
    println!("\n    Query into JSON: json_extract(data, '$.specs.battery')");
    let mut stmt = db.prepare(
        "SELECT id, json_extract(data, '$.name'), json_extract(data, '$.specs.battery') FROM products"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    {}: name={}, battery={:?}",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // Filter by JSON field
    println!("\n    SQL: SELECT ... WHERE json_extract(data, '$.price') > 10\n");
    let mut stmt = db.prepare(
        "SELECT id, json_extract(data, '$.name'), json_extract(data, '$.price')
         FROM products WHERE json_extract(data, '$.price') > 10"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    {}: {} (${:.2})",
            row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    println!("\n    Document store: flexible schema, embedded data, no JOINs.");
    println!("    In PostgreSQL: JSONB column. In MongoDB: native.");
    println!("    Best for: product catalogs, CMS, varying attributes.\n");
}
