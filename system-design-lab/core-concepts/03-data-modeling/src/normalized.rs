use rusqlite::Connection;

// =============================================================================
// Normalized (3NF) — separate tables, foreign keys, no duplicated data
//
//   users                    orders                  products
//   ┌────────────┐          ┌──────────────┐       ┌─────────────┐
//   │ id         │──┐       │ id           │   ┌───│ id          │
//   │ name       │  └──────►│ user_id (FK) │   │   │ name        │
//   │ email      │          │ product_id ──┼───┘   │ price       │
//   └────────────┘          │ quantity     │       └─────────────┘
//                           └──────────────┘
//
//   Reading: SELECT ... FROM orders JOIN users JOIN products
//            → must look up 3 tables to get one order's full info
//
//   Updating user name: UPDATE users SET name = 'X' WHERE id = 1
//            → ONE row, ONE table. ALL orders automatically reflect it.
//
//   Pros: no data duplication, easy updates, data integrity
//   Cons: JOINs are expensive at scale (each JOIN = extra lookup)
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Normalized (3NF) Relational Model ═══\n");

    let db = Connection::open_in_memory().unwrap();

    // Create normalized tables with foreign keys
    db.execute_batch(
        "
        CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL
        );
        CREATE TABLE products (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            price REAL NOT NULL
        );
        CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL REFERENCES users(id),
            product_id INTEGER NOT NULL REFERENCES products(id),
            quantity INTEGER NOT NULL
        );
    ",
    )
    .unwrap();

    // Insert data — each entity stored ONCE
    db.execute(
        "INSERT INTO users VALUES (1, 'Alice', 'alice@example.com')",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO users VALUES (2, 'Bob', 'bob@example.com')", [])
        .unwrap();
    db.execute("INSERT INTO products VALUES (1, 'Widget', 9.99)", [])
        .unwrap();
    db.execute("INSERT INTO products VALUES (2, 'Gadget', 24.99)", [])
        .unwrap();
    db.execute("INSERT INTO orders VALUES (1, 1, 1, 3)", [])
        .unwrap(); // Alice buys 3 Widgets
    db.execute("INSERT INTO orders VALUES (2, 1, 2, 1)", [])
        .unwrap(); // Alice buys 1 Gadget
    db.execute("INSERT INTO orders VALUES (3, 2, 1, 5)", [])
        .unwrap(); // Bob buys 5 Widgets

    // Reading requires JOINs
    println!("    SQL: SELECT ... FROM orders JOIN users JOIN products\n");
    let mut stmt = db
        .prepare(
            "
        SELECT o.id, u.name, o.quantity, p.name, p.price
        FROM orders o
        JOIN users u ON o.user_id = u.id
        JOIN products p ON o.product_id = p.id
    ",
        )
        .unwrap();

    let rows: Vec<String> = stmt
        .query_map([], |row| {
            Ok(format!(
                "    Order #{}: {} bought {}x {} (${:.2} each)",
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for row in &rows {
        println!("{}", row);
    }

    // Update user name — only ONE row in ONE table
    println!("\n    SQL: UPDATE users SET name = 'Alicia' WHERE id = 1\n");
    db.execute("UPDATE users SET name = 'Alicia' WHERE id = 1", [])
        .unwrap();

    let mut stmt = db
        .prepare(
            "
        SELECT o.id, u.name FROM orders o JOIN users u ON o.user_id = u.id WHERE u.id = 1
    ",
        )
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |row| {
            Ok(format!(
                "    Order #{}: now shows '{}'",
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for row in &rows {
        println!("{}", row);
    }
    println!("    → Both orders reflect new name (updated in ONE place).\n");
    println!("    Normalized: no duplication, easy updates, but JOINs cost time.");
    println!("    Best for: write-heavy, consistency-critical (banking, e-commerce).\n");
}
