use rusqlite::Connection;

// =============================================================================
// Event Sourcing — store ALL events, derive state by replaying
//
//   Instead of storing current state:
//     accounts: { id: 1, balance: 100 }   ← just the latest value
//
//   Store every event that ever happened:
//   ┌────┬──────────────────┬─────────────────────────────────┐
//   │ #  │ Event Type       │ Payload                          │
//   ├────┼──────────────────┼─────────────────────────────────┤
//   │ 1  │ AccountCreated   │ owner: "Alice"                   │
//   │ 2  │ Deposited        │ amount: 100                      │
//   │ 3  │ Withdrawn        │ amount: 30                       │
//   │ 4  │ Deposited        │ amount: 50                       │
//   │ 5  │ InterestApplied  │ rate: 5%                         │
//   │ 6  │ Withdrawn        │ amount: 20                       │
//   └────┴──────────────────┴─────────────────────────────────┘
//
//   Current balance = replay: 0 + 100 - 30 + 50 + interest - 20
//   Time travel: balance at event #3 = 0 + 100 - 30 = 70
//
//   Pros: full audit trail, can replay to any point, undo/redo
//   Cons: replaying gets slow → need periodic snapshots
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Event Sourcing ═══\n");

    let db = Connection::open_in_memory().unwrap();

    db.execute_batch("
        CREATE TABLE events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            amount REAL,
            rate REAL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        -- Append events (never UPDATE or DELETE!)
        INSERT INTO events (account_id, event_type, amount) VALUES (1, 'AccountCreated', 0);
        INSERT INTO events (account_id, event_type, amount) VALUES (1, 'Deposited', 100.0);
        INSERT INTO events (account_id, event_type, amount) VALUES (1, 'Withdrawn', 30.0);
        INSERT INTO events (account_id, event_type, amount) VALUES (1, 'Deposited', 50.0);
        INSERT INTO events (account_id, event_type, rate)   VALUES (1, 'InterestApplied', 0.05);
        INSERT INTO events (account_id, event_type, amount) VALUES (1, 'Withdrawn', 20.0);
    ").unwrap();

    // Show the event log
    println!("    Event log (append-only, never modified):\n");
    let mut stmt = db.prepare(
        "SELECT id, event_type, amount, rate, created_at FROM events ORDER BY id"
    ).unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let event_type: String = row.get(1)?;
        let amount: Option<f64> = row.get(2)?;
        let rate: Option<f64> = row.get(3)?;
        let ts: String = row.get(4)?;
        let detail = match (amount, rate) {
            (Some(a), _) => format!("amount={:.2}", a),
            (_, Some(r)) => format!("rate={:.0}%", r * 100.0),
            _ => String::new(),
        };
        Ok(format!("    #{} [{}] {} {}", id, ts, event_type, detail))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // Derive current balance by replaying ALL events (pure SQL!)
    println!("\n    Derive balance by replaying events (pure SQL):\n");
    println!("    SQL: SELECT SUM(CASE event_type ... END) FROM events\n");

    let balance: f64 = db.query_row("
        SELECT SUM(
            CASE event_type
                WHEN 'Deposited' THEN amount
                WHEN 'Withdrawn' THEN -amount
                ELSE 0
            END
        ) FROM events WHERE account_id = 1 AND event_type IN ('Deposited', 'Withdrawn')
    ", [], |r| r.get(0)).unwrap();
    println!("    Balance (deposits - withdrawals): ${:.2}", balance);

    // Running balance at each point — window function
    println!("\n    Time travel — balance after each event:\n");
    let mut stmt = db.prepare("
        SELECT id, event_type,
            COALESCE(amount, 0) as amt,
            SUM(CASE event_type
                WHEN 'Deposited' THEN amount
                WHEN 'Withdrawn' THEN -amount
                ELSE 0
            END) OVER (ORDER BY id) AS running_balance
        FROM events WHERE account_id = 1
    ").unwrap();
    let rows: Vec<String> = stmt.query_map([], |row| {
        Ok(format!("    After event #{} ({}): ${:.2}",
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(3)?))
    }).unwrap().filter_map(|r| r.ok()).collect();
    for row in &rows { println!("{}", row); }

    // Count events (in production, you'd snapshot periodically)
    let count: i64 = db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap();
    println!("\n    Total events: {} (in production: snapshot every N events)", count);

    println!("\n    Event sourcing: append-only log, derive state by replay.");
    println!("    Use cases: banking, audit logs, undo/redo, CQRS.\n");
}
