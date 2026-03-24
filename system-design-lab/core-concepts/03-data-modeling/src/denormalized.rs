use rusqlite::Connection;

// =============================================================================
// Denormalized — embed related data directly in one table, avoid JOINs
//
//   orders_denorm
//   ┌──────────────────────────────────────────┐
//   │ id                                       │
//   │ user_id                                  │
//   │ user_name       ← duplicated from users  │
//   │ user_email      ← duplicated from users  │
//   │ product_name    ← duplicated from prods  │
//   │ product_price   ← duplicated from prods  │
//   │ quantity                                  │
//   └──────────────────────────────────────────┘
//
//   Reading: SELECT * FROM orders_denorm WHERE id = 1
//            → everything in ONE row, no JOINs needed
//
//   Updating user name: UPDATE orders_denorm SET user_name = 'X' WHERE user_id = 1
//            → must find and update EVERY order for this user!
//
//   Pros: fast reads (no JOINs), single table scan
//   Cons: data duplication, update anomalies
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Denormalized Model ═══\n");

    let db = Connection::open_in_memory().unwrap();

    db.execute_batch("
        CREATE TABLE orders_denorm (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            user_name TEXT NOT NULL,
            user_email TEXT NOT NULL,
            product_name TEXT NOT NULL,
            product_price REAL NOT NULL,
            quantity INTEGER NOT NULL
        );
        INSERT INTO orders_denorm VALUES (1, 1, 'Alice', 'alice@example.com', 'Widget', 9.99, 3);
        INSERT INTO orders_denorm VALUES (2, 1, 'Alice', 'alice@example.com', 'Gadget', 24.99, 1);
        INSERT INTO orders_denorm VALUES (3, 1, 'Alice', 'alice@example.com', 'Widget', 9.99, 5);
        INSERT INTO orders_denorm VALUES (4, 2, 'Bob',   'bob@example.com',   'Widget', 9.99, 2);
    ").unwrap();

    println!("    SQL: SELECT * FROM orders_denorm (no JOINs needed)\n");
    let mut stmt = db.prepare(
        "SELECT id, user_name, quantity, product_name, product_price FROM orders_denorm"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    Order #{}: {} bought {}x {} (${:.2})",
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?, row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // Update is expensive — must find and fix ALL rows
    println!("\n    SQL: UPDATE orders_denorm SET user_name = 'Alicia' WHERE user_id = 1\n");
    let updated = db.execute(
        "UPDATE orders_denorm SET user_name = 'Alicia' WHERE user_id = 1", []
    ).unwrap();
    println!("    Updated {} rows! (scan every order for this user)\n", updated);

    let mut stmt = db.prepare(
        "SELECT id, user_name FROM orders_denorm WHERE user_id = 1"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    Order #{}: now shows '{}'",
            row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    println!("\n    Denormalized: fast reads, but updates touch many rows.");
    println!("    Best for: read-heavy (social feeds, dashboards).\n");
}
