use rusqlite::Connection;
use std::time::Instant;

// =============================================================================
// Key-Value Store — simple key → value lookups (like Redis, DynamoDB)
//
//   ┌─────────────────────┬──────────────────────────────┐
//   │ Key                  │ Value                         │
//   ├─────────────────────┼──────────────────────────────┤
//   │ session:abc123       │ {"user_id":42,"role":"admin"}│
//   │ cache:user:42        │ {"name":"Alice"} (TTL=60s)   │
//   │ likes:post:789       │ 42                           │
//   │ rate:1.2.3.4:min:5   │ 15 (rate limit counter)      │
//   └─────────────────────┴──────────────────────────────┘
//
//   Operations: GET key → O(1), SET key value → O(1)
//   Optional TTL: key auto-expires after N seconds
//
//   Pros: O(1) lookups, simple, blazing fast
//   Cons: no complex queries, no relationships, no JOINs
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Key-Value Store ═══\n");

    let db = Connection::open_in_memory().unwrap();

    db.execute_batch("
        CREATE TABLE kv (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            expires_at INTEGER  -- unix timestamp, NULL = no expiry
        );
    ").unwrap();

    // Session storage
    println!("    Use case 1: Session storage\n");
    db.execute(
        "INSERT INTO kv (key, value) VALUES ('session:abc123', '{\"user_id\":42,\"role\":\"admin\"}')",
        []
    ).unwrap();
    let val: String = db.query_row(
        "SELECT value FROM kv WHERE key = 'session:abc123'", [], |r| r.get(0)
    ).unwrap();
    println!("    SET session:abc123");
    println!("    GET session:abc123 → {}\n", val);

    // Cache with TTL
    println!("    Use case 2: Cache with TTL (expiry)\n");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;

    // Insert with expiry 2 seconds from now
    db.execute(
        "INSERT INTO kv (key, value, expires_at) VALUES ('cache:user:42', '{\"name\":\"Alice\"}', ?1)",
        [now + 2]
    ).unwrap();

    // Read before expiry
    let val: Option<String> = db.query_row(
        "SELECT value FROM kv WHERE key = 'cache:user:42' AND (expires_at IS NULL OR expires_at > ?1)",
        [now], |r| r.get(0)
    ).ok();
    println!("    GET cache:user:42 (before expiry) → {:?}", val);

    // Read after expiry (simulate by querying with future timestamp)
    let val: Option<String> = db.query_row(
        "SELECT value FROM kv WHERE key = 'cache:user:42' AND (expires_at IS NULL OR expires_at > ?1)",
        [now + 10], |r| r.get(0)
    ).ok();
    println!("    GET cache:user:42 (after expiry)  → {:?}  ← expired!\n", val);

    // Counter pattern (atomic increment)
    println!("    Use case 3: Counters (atomic updates)\n");
    db.execute("INSERT INTO kv (key, value) VALUES ('likes:post:789', '0')", []).unwrap();
    for _ in 0..5 {
        db.execute(
            "UPDATE kv SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'likes:post:789'",
            []
        ).unwrap();
    }
    let count: String = db.query_row(
        "SELECT value FROM kv WHERE key = 'likes:post:789'", [], |r| r.get(0)
    ).unwrap();
    println!("    After 5 increments: likes:post:789 = {}", count);

    // Batch lookup performance
    println!("\n    Use case 4: Batch inserts + lookup speed\n");
    let start = Instant::now();
    db.execute_batch("BEGIN").unwrap();
    for i in 0..10_000 {
        db.execute(
            "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
            rusqlite::params![format!("key:{}", i), format!("value-{}", i)]
        ).unwrap();
    }
    db.execute_batch("COMMIT").unwrap();
    println!("    Inserted 10,000 keys in {:?}", start.elapsed());

    let start = Instant::now();
    let _: String = db.query_row(
        "SELECT value FROM kv WHERE key = 'key:5000'", [], |r| r.get(0)
    ).unwrap();
    println!("    Lookup key:5000 → {:?} (primary key = instant)", start.elapsed());

    println!("\n    Key-value: O(1) lookups via primary key.");
    println!("    In production: Redis (in-memory), DynamoDB (managed).\n");
}
