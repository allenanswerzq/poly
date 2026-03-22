//! # Mini Redis Implementation
//!
//! A simplified Redis demonstrating core concepts:
//! - In-memory key-value storage
//! - Multiple data types (String, Hash, List)
//! - TTL/Expiration
//! - Pub/Sub

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Data Types
// =============================================================================

#[derive(Debug, Clone)]
pub enum RedisValue {
    String(String),
    Hash(HashMap<String, String>),
    List(VecDeque<String>),
    Set(std::collections::HashSet<String>),
}

struct Entry {
    value: RedisValue,
    expires_at: Option<Instant>,
}

impl Entry {
    fn new(value: RedisValue) -> Self {
        Self {
            value,
            expires_at: None,
        }
    }

    fn with_ttl(mut self, ttl: Duration) -> Self {
        self.expires_at = Some(Instant::now() + ttl);
        self
    }

    fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| exp < Instant::now())
            .unwrap_or(false)
    }
}

// =============================================================================
// Mini Redis
// =============================================================================

pub struct MiniRedis {
    data: DashMap<String, Entry>,
    pubsub: Arc<PubSub>,
}

impl MiniRedis {
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
            pubsub: Arc::new(PubSub::new()),
        }
    }

    // ===== String Commands =====

    /// SET key value [EX seconds]
    pub fn set(&self, key: &str, value: &str, ttl: Option<Duration>) -> &'static str {
        let entry = Entry::new(RedisValue::String(value.to_string()));
        let entry = match ttl {
            Some(t) => entry.with_ttl(t),
            None => entry,
        };
        self.data.insert(key.to_string(), entry);
        "OK"
    }

    /// GET key
    pub fn get(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(|entry| {
            if entry.is_expired() {
                drop(entry);
                self.data.remove(key);
                None
            } else if let RedisValue::String(s) = &entry.value {
                Some(s.clone())
            } else {
                None
            }
        })
    }

    /// DEL key
    pub fn del(&self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }

    /// INCR key
    pub fn incr(&self, key: &str) -> Result<i64, &'static str> {
        let mut entry = self.data.entry(key.to_string()).or_insert_with(|| {
            Entry::new(RedisValue::String("0".to_string()))
        });

        match &mut entry.value {
            RedisValue::String(s) => {
                let num: i64 = s.parse().map_err(|_| "ERR value is not an integer")?;
                let new_val = num + 1;
                *s = new_val.to_string();
                Ok(new_val)
            }
            _ => Err("WRONGTYPE Operation against a key holding the wrong kind of value"),
        }
    }

    /// EXPIRE key seconds
    pub fn expire(&self, key: &str, seconds: u64) -> bool {
        if let Some(mut entry) = self.data.get_mut(key) {
            entry.expires_at = Some(Instant::now() + Duration::from_secs(seconds));
            true
        } else {
            false
        }
    }

    /// TTL key
    pub fn ttl(&self, key: &str) -> i64 {
        match self.data.get(key) {
            None => -2,  // Key doesn't exist
            Some(entry) => match entry.expires_at {
                None => -1,  // No expiration
                Some(exp) => {
                    let now = Instant::now();
                    if exp <= now {
                        -2  // Expired
                    } else {
                        (exp - now).as_secs() as i64
                    }
                }
            }
        }
    }

    // ===== Hash Commands =====

    /// HSET key field value
    pub fn hset(&self, key: &str, field: &str, value: &str) -> i64 {
        let mut entry = self.data.entry(key.to_string()).or_insert_with(|| {
            Entry::new(RedisValue::Hash(HashMap::new()))
        });

        if let RedisValue::Hash(ref mut hash) = entry.value {
            let is_new = !hash.contains_key(field);
            hash.insert(field.to_string(), value.to_string());
            if is_new { 1 } else { 0 }
        } else {
            0  // Wrong type
        }
    }

    /// HGET key field
    pub fn hget(&self, key: &str, field: &str) -> Option<String> {
        self.data.get(key).and_then(|entry| {
            if let RedisValue::Hash(hash) = &entry.value {
                hash.get(field).cloned()
            } else {
                None
            }
        })
    }

    /// HGETALL key
    pub fn hgetall(&self, key: &str) -> Vec<(String, String)> {
        self.data.get(key).map(|entry| {
            if let RedisValue::Hash(hash) = &entry.value {
                hash.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else {
                vec![]
            }
        }).unwrap_or_default()
    }

    // ===== List Commands =====

    /// LPUSH key value
    pub fn lpush(&self, key: &str, value: &str) -> usize {
        let mut entry = self.data.entry(key.to_string()).or_insert_with(|| {
            Entry::new(RedisValue::List(VecDeque::new()))
        });

        if let RedisValue::List(ref mut list) = entry.value {
            list.push_front(value.to_string());
            list.len()
        } else {
            0
        }
    }

    /// RPUSH key value
    pub fn rpush(&self, key: &str, value: &str) -> usize {
        let mut entry = self.data.entry(key.to_string()).or_insert_with(|| {
            Entry::new(RedisValue::List(VecDeque::new()))
        });

        if let RedisValue::List(ref mut list) = entry.value {
            list.push_back(value.to_string());
            list.len()
        } else {
            0
        }
    }

    /// LPOP key
    pub fn lpop(&self, key: &str) -> Option<String> {
        self.data.get_mut(key).and_then(|mut entry| {
            if let RedisValue::List(ref mut list) = entry.value {
                list.pop_front()
            } else {
                None
            }
        })
    }

    /// RPOP key
    pub fn rpop(&self, key: &str) -> Option<String> {
        self.data.get_mut(key).and_then(|mut entry| {
            if let RedisValue::List(ref mut list) = entry.value {
                list.pop_back()
            } else {
                None
            }
        })
    }

    /// LRANGE key start stop
    pub fn lrange(&self, key: &str, start: i64, stop: i64) -> Vec<String> {
        self.data.get(key).map(|entry| {
            if let RedisValue::List(list) = &entry.value {
                let len = list.len() as i64;
                let start = if start < 0 { (len + start).max(0) } else { start } as usize;
                let stop = if stop < 0 { (len + stop).max(0) } else { stop } as usize;

                list.iter()
                    .skip(start)
                    .take(stop - start + 1)
                    .cloned()
                    .collect()
            } else {
                vec![]
            }
        }).unwrap_or_default()
    }

    // ===== Set Commands =====

    /// SADD key member
    pub fn sadd(&self, key: &str, member: &str) -> i64 {
        let mut entry = self.data.entry(key.to_string()).or_insert_with(|| {
            Entry::new(RedisValue::Set(std::collections::HashSet::new()))
        });

        if let RedisValue::Set(ref mut set) = entry.value {
            if set.insert(member.to_string()) { 1 } else { 0 }
        } else {
            0
        }
    }

    /// SMEMBERS key
    pub fn smembers(&self, key: &str) -> Vec<String> {
        self.data.get(key).map(|entry| {
            if let RedisValue::Set(set) = &entry.value {
                set.iter().cloned().collect()
            } else {
                vec![]
            }
        }).unwrap_or_default()
    }

    // ===== Pub/Sub =====

    pub fn subscribe(&self, channel: &str) -> Receiver {
        self.pubsub.subscribe(channel)
    }

    pub fn publish(&self, channel: &str, message: &str) -> usize {
        self.pubsub.publish(channel, message)
    }

    // ===== Utility =====

    pub fn keys(&self, pattern: &str) -> Vec<String> {
        // Simple pattern matching (only supports * wildcard at end)
        self.data.iter()
            .filter(|entry| !entry.is_expired())
            .filter(|entry| {
                if pattern == "*" {
                    true
                } else if pattern.ends_with('*') {
                    entry.key().starts_with(&pattern[..pattern.len()-1])
                } else {
                    entry.key() == pattern
                }
            })
            .map(|entry| entry.key().clone())
            .collect()
    }

    pub fn dbsize(&self) -> usize {
        self.data.iter().filter(|e| !e.is_expired()).count()
    }

    pub fn flushdb(&self) {
        self.data.clear();
    }
}

impl Default for MiniRedis {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Pub/Sub
// =============================================================================

pub struct PubSub {
    channels: DashMap<String, Vec<Arc<Mutex<VecDeque<String>>>>>,
}

pub struct Receiver {
    queue: Arc<Mutex<VecDeque<String>>>,
}

impl Receiver {
    pub fn recv(&self) -> Option<String> {
        self.queue.lock().pop_front()
    }
}

impl PubSub {
    pub fn new() -> Self {
        Self {
            channels: DashMap::new(),
        }
    }

    pub fn subscribe(&self, channel: &str) -> Receiver {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        self.channels
            .entry(channel.to_string())
            .or_default()
            .push(Arc::clone(&queue));

        Receiver { queue }
    }

    pub fn publish(&self, channel: &str, message: &str) -> usize {
        if let Some(subscribers) = self.channels.get(channel) {
            let count = subscribers.len();
            for sub in subscribers.iter() {
                sub.lock().push_back(message.to_string());
            }
            count
        } else {
            0
        }
    }
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== Mini Redis Demo ===\n");

    let redis = MiniRedis::new();

    // String commands
    println!("\n  ═══ String Commands ═══");
    redis.set("name", "Alice", None);
    println!("SET name 'Alice': OK");
    println!("GET name: {:?}", redis.get("name"));

    redis.set("counter", "0", None);
    println!("\nINCR counter 3 times:");
    for _ in 0..3 {
        let val = redis.incr("counter").unwrap();
        println!("  counter = {}", val);
    }

    // TTL/Expiration
    println!("\n--- TTL/Expiration ---");
    redis.set("session", "data", Some(Duration::from_secs(10)));
    println!("SET session 'data' EX 10");
    println!("TTL session: {} seconds", redis.ttl("session"));

    redis.expire("name", 60);
    println!("EXPIRE name 60");
    println!("TTL name: {} seconds", redis.ttl("name"));

    // Hash commands
    println!("\n--- Hash Commands ---");
    redis.hset("user:1", "name", "Bob");
    redis.hset("user:1", "age", "30");
    redis.hset("user:1", "city", "NYC");
    println!("HSET user:1 name Bob, age 30, city NYC");

    println!("HGET user:1 name: {:?}", redis.hget("user:1", "name"));
    println!("HGETALL user:1: {:?}", redis.hgetall("user:1"));

    // List commands
    println!("\n--- List Commands ---");
    redis.rpush("queue", "task1");
    redis.rpush("queue", "task2");
    redis.rpush("queue", "task3");
    println!("RPUSH queue task1 task2 task3");

    println!("LRANGE queue 0 -1: {:?}", redis.lrange("queue", 0, -1));
    println!("LPOP queue: {:?}", redis.lpop("queue"));
    println!("LRANGE queue 0 -1: {:?}", redis.lrange("queue", 0, -1));

    // Set commands
    println!("\n--- Set Commands ---");
    redis.sadd("tags", "rust");
    redis.sadd("tags", "redis");
    redis.sadd("tags", "cache");
    redis.sadd("tags", "rust");  // Duplicate, won't be added
    println!("SADD tags rust redis cache rust");
    println!("SMEMBERS tags: {:?}", redis.smembers("tags"));

    // Pub/Sub
    println!("\n--- Pub/Sub ---");
    let receiver = redis.subscribe("news");
    let num = redis.publish("news", "Breaking news!");
    println!("PUBLISH news 'Breaking news!' -> {} subscribers", num);
    println!("Subscriber received: {:?}", receiver.recv());

    // Utility commands
    println!("\n--- Utility Commands ---");
    println!("KEYS *: {:?}", redis.keys("*"));
    println!("KEYS user:*: {:?}", redis.keys("user:*"));
    println!("DBSIZE: {}", redis.dbsize());

    // Demo: Rate limiting pattern
    println!("\n--- Rate Limiting Pattern ---");
    let key = "ratelimit:user:123:minute";
    redis.set(key, "0", Some(Duration::from_secs(60)));

    for i in 1..=5 {
        let count = redis.incr(key).unwrap();
        let allowed = count <= 3;
        println!("  Request {}: count={}, allowed={}", i, count, allowed);
    }

    // Demo: Cache pattern
    println!("\n--- Caching Pattern ---");
    fn get_user(redis: &MiniRedis, id: u32) -> String {
        let key = format!("user:{}", id);

        // Try cache first
        if let Some(cached) = redis.get(&key) {
            println!("  Cache HIT for {}", key);
            return cached;
        }

        // Miss - "fetch from database"
        println!("  Cache MISS for {} - fetching from DB", key);
        let user_data = format!("{{\"id\": {}, \"name\": \"User{}\"}}", id, id);

        // Store in cache with 1 hour TTL
        redis.set(&key, &user_data, Some(Duration::from_secs(3600)));

        user_data
    }

    get_user(&redis, 100);  // Miss
    get_user(&redis, 100);  // Hit
    get_user(&redis, 200);  // Miss
    get_user(&redis, 100);  // Hit

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_operations() {
        let redis = MiniRedis::new();

        redis.set("key", "value", None);
        assert_eq!(redis.get("key"), Some("value".to_string()));

        redis.del("key");
        assert_eq!(redis.get("key"), None);
    }

    #[test]
    fn test_incr() {
        let redis = MiniRedis::new();

        assert_eq!(redis.incr("counter").unwrap(), 1);
        assert_eq!(redis.incr("counter").unwrap(), 2);
        assert_eq!(redis.incr("counter").unwrap(), 3);
    }

    #[test]
    fn test_hash_operations() {
        let redis = MiniRedis::new();

        redis.hset("h", "f1", "v1");
        redis.hset("h", "f2", "v2");

        assert_eq!(redis.hget("h", "f1"), Some("v1".to_string()));
        assert_eq!(redis.hgetall("h").len(), 2);
    }

    #[test]
    fn test_list_operations() {
        let redis = MiniRedis::new();

        redis.rpush("list", "a");
        redis.rpush("list", "b");
        redis.lpush("list", "z");

        assert_eq!(redis.lrange("list", 0, -1), vec!["z", "a", "b"]);
        assert_eq!(redis.lpop("list"), Some("z".to_string()));
        assert_eq!(redis.rpop("list"), Some("b".to_string()));
    }

    #[test]
    fn test_expiration() {
        let redis = MiniRedis::new();

        redis.set("temp", "data", Some(Duration::from_millis(50)));
        assert!(redis.get("temp").is_some());

        std::thread::sleep(Duration::from_millis(60));
        assert!(redis.get("temp").is_none());
    }

    #[test]
    fn test_pubsub() {
        let redis = MiniRedis::new();

        let rx = redis.subscribe("channel");
        redis.publish("channel", "message");

        assert_eq!(rx.recv(), Some("message".to_string()));
    }
}
