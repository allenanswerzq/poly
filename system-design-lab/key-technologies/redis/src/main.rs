// =============================================================================
// Demonstration
// =============================================================================

use mini_redis::MiniRedis;
use std::time::Duration;
use std::thread;

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
