#![allow(dead_code, unused_variables, unused_imports, clippy::all)]
//! # Mini Redis Implementation
//!
//! A simplified Redis demonstrating core concepts:
//! - In-memory key-value storage
//! - Multiple data types (String, Hash, List)
//! - TTL/Expiration
//! - Pub/Sub

use dashmap::DashMap;
use parking_lot::Mutex;
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
        let mut entry = self
            .data
            .entry(key.to_string())
            .or_insert_with(|| Entry::new(RedisValue::String("0".to_string())));

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
            None => -2, // Key doesn't exist
            Some(entry) => match entry.expires_at {
                None => -1, // No expiration
                Some(exp) => {
                    let now = Instant::now();
                    if exp <= now {
                        -2 // Expired
                    } else {
                        (exp - now).as_secs() as i64
                    }
                }
            },
        }
    }

    // ===== Hash Commands =====

    /// HSET key field value
    pub fn hset(&self, key: &str, field: &str, value: &str) -> i64 {
        let mut entry = self
            .data
            .entry(key.to_string())
            .or_insert_with(|| Entry::new(RedisValue::Hash(HashMap::new())));

        if let RedisValue::Hash(ref mut hash) = entry.value {
            let is_new = !hash.contains_key(field);
            hash.insert(field.to_string(), value.to_string());
            if is_new {
                1
            } else {
                0
            }
        } else {
            0 // Wrong type
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
        self.data
            .get(key)
            .map(|entry| {
                if let RedisValue::Hash(hash) = &entry.value {
                    hash.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
    }

    // ===== List Commands =====

    /// LPUSH key value
    pub fn lpush(&self, key: &str, value: &str) -> usize {
        let mut entry = self
            .data
            .entry(key.to_string())
            .or_insert_with(|| Entry::new(RedisValue::List(VecDeque::new())));

        if let RedisValue::List(ref mut list) = entry.value {
            list.push_front(value.to_string());
            list.len()
        } else {
            0
        }
    }

    /// RPUSH key value
    pub fn rpush(&self, key: &str, value: &str) -> usize {
        let mut entry = self
            .data
            .entry(key.to_string())
            .or_insert_with(|| Entry::new(RedisValue::List(VecDeque::new())));

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
        self.data
            .get(key)
            .map(|entry| {
                if let RedisValue::List(list) = &entry.value {
                    let len = list.len() as i64;
                    let start = if start < 0 {
                        (len + start).max(0)
                    } else {
                        start
                    } as usize;
                    let stop = if stop < 0 { (len + stop).max(0) } else { stop } as usize;

                    list.iter()
                        .skip(start)
                        .take(stop - start + 1)
                        .cloned()
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
    }

    // ===== Set Commands =====

    /// SADD key member
    pub fn sadd(&self, key: &str, member: &str) -> i64 {
        let mut entry = self
            .data
            .entry(key.to_string())
            .or_insert_with(|| Entry::new(RedisValue::Set(std::collections::HashSet::new())));

        if let RedisValue::Set(ref mut set) = entry.value {
            if set.insert(member.to_string()) {
                1
            } else {
                0
            }
        } else {
            0
        }
    }

    /// SMEMBERS key
    pub fn smembers(&self, key: &str) -> Vec<String> {
        self.data
            .get(key)
            .map(|entry| {
                if let RedisValue::Set(set) = &entry.value {
                    set.iter().cloned().collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default()
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
        self.data
            .iter()
            .filter(|entry| !entry.is_expired())
            .filter(|entry| {
                if pattern == "*" {
                    true
                } else if pattern.ends_with('*') {
                    entry.key().starts_with(&pattern[..pattern.len() - 1])
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

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
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
