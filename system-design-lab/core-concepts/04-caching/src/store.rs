use redis::Commands;
use rusqlite::Connection as SqliteConn;
use std::sync::{Arc, Mutex};

// =============================================================================
// Clean wrappers over Redis + SQLite
//
// Hides all the .lock().unwrap().query().ok().flatten() noise.
// Demo code reads like pseudocode:
//
//   let store = Store::new();
//   store.cache_set("key", "value", 60);
//   store.cache_get("key");       → Some("value")
//   store.db_set("key", "value");
//   store.db_get("key");          → Some("value")
// =============================================================================

/// Redis wrapper — clean API over redis-rs
pub struct Cache {
    conn: Mutex<redis::Connection>,
}

impl Cache {
    pub fn new(conn: redis::Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// SET key value EX ttl_secs
    pub fn set(&self, key: &str, value: &str, ttl_secs: u64) {
        let mut r = self.conn.lock().unwrap();
        let _: Result<(), _> = r.set_ex(key, value, ttl_secs);
    }

    /// SET key value (no TTL)
    pub fn set_permanent(&self, key: &str, value: &str) {
        let mut r = self.conn.lock().unwrap();
        let _: Result<(), _> = r.set::<_, _, ()>(key, value);
    }

    /// GET key → Option<String>
    pub fn get(&self, key: &str) -> Option<String> {
        let mut r = self.conn.lock().unwrap();
        r.get(key).ok().flatten()
    }

    /// DEL key
    pub fn del(&self, key: &str) {
        let mut r = self.conn.lock().unwrap();
        let _: Result<(), _> = r.del(key);
    }

    /// TTL key → seconds remaining (-1 = no TTL, -2 = key doesn't exist)
    pub fn ttl(&self, key: &str) -> i64 {
        let mut r = self.conn.lock().unwrap();
        redis::cmd("TTL").arg(key).query(&mut *r).unwrap_or(-2)
    }

    /// SETNX (acquire lock): returns true if lock acquired
    pub fn try_lock(&self, key: &str, ttl_secs: u64) -> bool {
        let mut r = self.conn.lock().unwrap();
        redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query(&mut *r)
            .unwrap_or(false)
    }

    /// Pipeline SET: batch multiple sets in 1 round-trip
    pub fn set_batch(&self, entries: &[(&str, &str)], ttl_secs: u64) {
        let mut r = self.conn.lock().unwrap();
        let mut pipe = redis::pipe();
        for (k, v) in entries {
            pipe.set_ex(*k, *v, ttl_secs);
        }
        let _: Result<(), _> = pipe.query(&mut *r);
    }

    /// FLUSHDB
    pub fn flush(&self) {
        let mut r = self.conn.lock().unwrap();
        let _: Result<(), _> = redis::cmd("FLUSHDB").query(&mut *r);
    }

    /// EXPIRE key seconds
    pub fn expire(&self, key: &str, secs: u64) {
        let mut r = self.conn.lock().unwrap();
        let _: Result<(), _> = redis::cmd("EXPIRE").arg(key).arg(secs).query(&mut *r);
    }

    /// PERSIST key (remove TTL)
    pub fn persist(&self, key: &str) {
        let mut r = self.conn.lock().unwrap();
        let _: Result<(), _> = redis::cmd("PERSIST").arg(key).query(&mut *r);
    }

    /// SMEMBERS set_key — get all members of a Redis SET
    pub fn smembers(&self, set_key: &str) -> Vec<String> {
        let mut r = self.conn.lock().unwrap();
        redis::cmd("SMEMBERS")
            .arg(set_key)
            .query(&mut *r)
            .unwrap_or_default()
    }

    /// SCARD set_key — number of members in a Redis SET
    pub fn scard(&self, set_key: &str) -> i64 {
        let mut r = self.conn.lock().unwrap();
        redis::cmd("SCARD").arg(set_key).query(&mut *r).unwrap_or(0)
    }

    /// Pipeline: SET key value EX ttl + SADD set_key key — atomic in 1 RTT.
    /// Ensures the value and dirty marker are both written or neither.
    pub fn set_and_sadd(&self, key: &str, value: &str, ttl_secs: u64, set_key: &str) {
        let mut r = self.conn.lock().unwrap();
        let mut pipe = redis::pipe();
        pipe.atomic() // MULTI/EXEC — all or nothing
            .set_ex(key, value, ttl_secs)
            .cmd("SADD")
            .arg(set_key)
            .arg(key);
        let _: Result<(), _> = pipe.query(&mut *r);
    }

    /// RENAME old_key new_key — atomic rename. Fails silently if old_key missing.
    pub fn rename(&self, old_key: &str, new_key: &str) -> bool {
        let mut r = self.conn.lock().unwrap();
        redis::cmd("RENAME")
            .arg(old_key)
            .arg(new_key)
            .query::<String>(&mut *r)
            .is_ok()
    }
}

/// SQLite wrapper — clean API for a simple key-value table
pub struct Db {
    conn: Mutex<SqliteConn>,
}

impl Db {
    pub fn new() -> Self {
        let conn = SqliteConn::open_in_memory().unwrap();
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        ",
        )
        .unwrap();
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// UPSERT: insert or update
    pub fn set(&self, key: &str, value: &str) {
        let db = self.conn.lock().unwrap();
        db.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )
        .unwrap();
    }

    /// SELECT value WHERE key = ?
    pub fn get(&self, key: &str) -> Option<String> {
        let db = self.conn.lock().unwrap();
        db.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .ok()
    }

    /// DELETE WHERE key = ?
    pub fn del(&self, key: &str) {
        let db = self.conn.lock().unwrap();
        db.execute("DELETE FROM kv WHERE key = ?1", [key]).unwrap();
    }

    /// COUNT(*)
    pub fn count(&self) -> i64 {
        let db = self.conn.lock().unwrap();
        db.query_row("SELECT COUNT(*) FROM kv", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// Batch insert in a transaction
    #[allow(dead_code)]
    pub fn set_batch(&self, entries: &[(&str, &str)]) {
        let db = self.conn.lock().unwrap();
        db.execute_batch("BEGIN").unwrap();
        for (k, v) in entries {
            db.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                rusqlite::params![k, v],
            ).unwrap();
        }
        db.execute_batch("COMMIT").unwrap();
    }

    /// Execute raw SQL
    #[allow(dead_code)]
    pub fn exec(&self, sql: &str) {
        let db = self.conn.lock().unwrap();
        db.execute_batch(sql).unwrap();
    }

    /// Query a single string value
    #[allow(dead_code)]
    pub fn query_one(&self, sql: &str, params: &[&str]) -> Option<String> {
        let db = self.conn.lock().unwrap();
        let params: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        db.query_row(sql, params.as_slice(), |r| r.get(0)).ok()
    }
}

/// A combined store with both Cache (Redis) and Db (SQLite)
pub struct Store {
    pub cache: Arc<Cache>,
    pub db: Arc<Db>,
    pub _server: crate::redis_server::RedisServer,
}

impl Store {
    pub fn new() -> Self {
        let server = crate::redis_server::RedisServer::start();
        let cache = Arc::new(Cache::new(server.connect()));
        let db = Arc::new(Db::new());
        Self {
            cache,
            db,
            _server: server,
        }
    }

    /// Get a new Redis connection to the same server (for multi-threaded use)
    pub fn new_cache_conn(&self) -> Cache {
        Cache::new(self._server.connect())
    }
}
