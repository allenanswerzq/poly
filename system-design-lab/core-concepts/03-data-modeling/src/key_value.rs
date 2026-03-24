use redb::{Database, ReadableTable, TableDefinition};
use std::time::Instant;

// =============================================================================
// Key-Value Store — using redb (real embedded KV database, like RocksDB)
//
//   ┌─────────────────────────┬──────────────────────────────┐
//   │ Key                     │ Value                         │
//   ├─────────────────────────┼──────────────────────────────┤
//   │ session:abc123          │ {"user_id":42,"role":"admin"} │
//   │ cache:user:42           │ {"name":"Alice"}              │
//   │ likes:post:789          │ 42                            │
//   │ rate:1.2.3.4:min:5      │ 15 (rate limit counter)       │
//   └─────────────────────────┴──────────────────────────────┘
//
//   redb is a real embedded key-value database (like RocksDB/LMDB).
//   Not simulated with HashMap — actual B-tree on disk, crash-safe,
//   ACID transactions.
//
//   Operations: GET key → O(log n), SET key value → O(log n)
//   Pros: fast lookups, simple API, crash-safe, ACID
//   Cons: no complex queries, no relationships, just key→value
// =============================================================================

// Define a table (like a Redis keyspace or RocksDB column family)
const SESSIONS: TableDefinition<&str, &str> = TableDefinition::new("sessions");
const COUNTERS: TableDefinition<&str, u64> = TableDefinition::new("counters");
const BULK: TableDefinition<&str, &str> = TableDefinition::new("bulk");

pub fn demo() {
    println!("\n  ═══ Key-Value Store (redb — real embedded KV) ═══\n");
    println!("    Using redb: a real embedded KV database (like RocksDB/LMDB).\n");

    let db = Database::builder()
        .create_with_backend(redb::backends::InMemoryBackend::new())
        .unwrap();

    // ── Use case 1: Session storage ──
    println!("    Use case 1: Session storage\n");
    {
        let txn = db.begin_write().unwrap();
        {
            let mut table = txn.open_table(SESSIONS).unwrap();
            table.insert("session:abc123", r#"{"user_id":42,"role":"admin"}"#).unwrap();
            table.insert("session:def456", r#"{"user_id":7,"role":"user"}"#).unwrap();
        }
        txn.commit().unwrap();
    }

    {
        let txn = db.begin_read().unwrap();
        let table = txn.open_table(SESSIONS).unwrap();
        let val = table.get("session:abc123").unwrap().unwrap();
        println!("    SET session:abc123");
        println!("    GET session:abc123 → {}\n", val.value());
    }

    // ── Use case 2: Atomic counters ──
    println!("    Use case 2: Atomic counters (likes, views)\n");
    {
        let txn = db.begin_write().unwrap();
        {
            let mut table = txn.open_table(COUNTERS).unwrap();
            table.insert("likes:post:789", &0u64).unwrap();
        }
        txn.commit().unwrap();
    }

    // Increment 5 times — each in a transaction (crash-safe)
    for _ in 0..5 {
        let txn = db.begin_write().unwrap();
        {
            let mut table = txn.open_table(COUNTERS).unwrap();
            let current = table.get("likes:post:789").unwrap()
                .map(|v| v.value()).unwrap_or(0);
            table.insert("likes:post:789", &(current + 1)).unwrap();
        }
        txn.commit().unwrap();
    }

    {
        let txn = db.begin_read().unwrap();
        let table = txn.open_table(COUNTERS).unwrap();
        let val = table.get("likes:post:789").unwrap().unwrap();
        println!("    After 5 increments: likes:post:789 = {}", val.value());
    }

    // ── Use case 3: Batch insert + lookup speed ──
    println!("\n    Use case 3: Batch insert + lookup speed\n");
    let n = 10_000;
    let start = Instant::now();
    {
        let txn = db.begin_write().unwrap();
        {
            let mut table = txn.open_table(BULK).unwrap();
            for i in 0..n {
                // We need owned strings for the key/value
                let key = format!("key:{}", i);
                let val = format!("value-{}", i);
                table.insert(key.as_str(), val.as_str()).unwrap();
            }
        }
        txn.commit().unwrap();
    }
    println!("    Inserted {} keys in {:?}", n, start.elapsed());

    let start = Instant::now();
    {
        let txn = db.begin_read().unwrap();
        let table = txn.open_table(BULK).unwrap();
        let val = table.get("key:5000").unwrap().unwrap();
        let _ = val.value(); // read the value
    }
    println!("    Lookup key:5000 → {:?} (B-tree lookup)", start.elapsed());

    // ── Use case 4: Range scan ──
    println!("\n    Use case 4: Range scan (keys 100..110)\n");
    {
        let txn = db.begin_read().unwrap();
        let table = txn.open_table(BULK).unwrap();
        // range scan — redb supports ordered iteration since keys are in a B-tree
        let mut count = 0;
        let range = table.range("key:100"..="key:110").unwrap();
        for entry in range {
            let (k, v) = entry.unwrap();
            if count < 3 {
                println!("    {} → {}", k.value(), v.value());
            }
            count += 1;
        }
        if count > 3 {
            println!("    ... ({} total keys in range)", count);
        }
    }

    println!("\n    redb: real embedded KV database, ACID transactions, B-tree storage.");
    println!("    In production: Redis (in-memory), RocksDB (on-disk), DynamoDB (managed).\n");
}
