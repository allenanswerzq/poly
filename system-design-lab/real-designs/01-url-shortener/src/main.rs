#![allow(dead_code, unused_variables, unused_imports)]
//! # URL Shortener Implementation
//!
//! A complete URL shortener demonstrating:
//! - Multiple ID generation strategies
//! - In-memory storage with sharding simulation
//! - Caching layer
//! - Analytics tracking

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Base62 characters for URL-safe encoding
const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

// =============================================================================
// ID Generation Strategies
// =============================================================================

/// Encode a number to base62 string
fn encode_base62(mut num: u64) -> String {
    if num == 0 {
        return "0".to_string();
    }

    let mut result = Vec::new();
    while num > 0 {
        result.push(BASE62[(num % 62) as usize]);
        num /= 62;
    }
    result.reverse();
    String::from_utf8(result).unwrap()
}

/// Strategy 1: Hash-based ID generation
pub fn generate_hash_id(url: &str, length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    let result = hasher.finalize();

    // Take first 8 bytes as u64, then encode to base62
    let num = u64::from_be_bytes(result[0..8].try_into().unwrap());
    let encoded = encode_base62(num);

    // Return requested length
    encoded.chars().take(length).collect()
}

/// Strategy 2: Counter-based ID generation
pub struct CounterIdGenerator {
    counter: AtomicU64,
    prefix: u8, // Server ID for distributed uniqueness
}

impl CounterIdGenerator {
    pub fn new(server_id: u8) -> Self {
        Self {
            counter: AtomicU64::new(0),
            prefix: server_id,
        }
    }

    pub fn generate(&self) -> String {
        let count = self.counter.fetch_add(1, Ordering::SeqCst);
        // Combine server prefix with counter
        let id = ((self.prefix as u64) << 56) | count;
        encode_base62(id)
    }
}

/// Strategy 3: Random ID with collision detection
pub fn generate_random_id(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| BASE62[rng.gen_range(0..62)] as char)
        .collect()
}

// =============================================================================
// Data Models
// =============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct Url {
    pub short_code: String,
    pub long_url: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub click_count: u64,
    pub user_id: Option<String>,
}

impl Clone for Url {
    fn clone(&self) -> Self {
        Self {
            short_code: self.short_code.clone(),
            long_url: self.long_url.clone(),
            created_at: self.created_at,
            expires_at: self.expires_at,
            click_count: self.click_count,
            user_id: self.user_id.clone(),
        }
    }
}

impl Url {
    pub fn new(short_code: String, long_url: String) -> Self {
        Self {
            short_code,
            long_url,
            created_at: Utc::now(),
            expires_at: None,
            click_count: 0,
            user_id: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickEvent {
    pub short_code: String,
    pub timestamp: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub referrer: Option<String>,
    pub ip_address: Option<String>,
}

// =============================================================================
// URL Shortener Service
// =============================================================================

/// The main URL shortener service
pub struct UrlShortener {
    /// Primary storage (simulates sharded database)
    urls: DashMap<String, Url>,
    /// Cache for hot URLs
    cache: DashMap<String, (Url, Instant)>,
    /// Cache TTL
    cache_ttl: Duration,
    /// ID generator
    id_generator: IdGeneratorType,
    /// Analytics buffer
    clicks: DashMap<String, Vec<ClickEvent>>,
}

pub enum IdGeneratorType {
    Hash,
    Counter(CounterIdGenerator),
    Random,
}

impl UrlShortener {
    pub fn new(id_type: IdGeneratorType) -> Self {
        Self {
            urls: DashMap::new(),
            cache: DashMap::new(),
            cache_ttl: Duration::from_secs(300), // 5 minute cache
            id_generator: id_type,
            clicks: DashMap::new(),
        }
    }

    /// Shorten a URL
    pub fn shorten(
        &self,
        long_url: &str,
        custom_alias: Option<&str>,
    ) -> Result<String, &'static str> {
        // Check if URL already exists (deduplication)
        for entry in self.urls.iter() {
            if entry.value().long_url == long_url {
                return Ok(entry.key().clone());
            }
        }

        // Generate or use custom code
        let short_code = if let Some(alias) = custom_alias {
            // Check if alias is available
            if self.urls.contains_key(alias) {
                return Err("Custom alias already taken");
            }
            alias.to_string()
        } else {
            self.generate_unique_code(long_url)?
        };

        // Store URL
        let url = Url::new(short_code.clone(), long_url.to_string());
        self.urls.insert(short_code.clone(), url);

        Ok(short_code)
    }

    /// Generate a unique short code
    fn generate_unique_code(&self, long_url: &str) -> Result<String, &'static str> {
        let max_attempts = 10;

        for attempt in 0..max_attempts {
            let code = match &self.id_generator {
                IdGeneratorType::Hash => {
                    // Add attempt to avoid same hash collision
                    let input = format!("{}{}", long_url, attempt);
                    generate_hash_id(&input, 7)
                }
                IdGeneratorType::Counter(gen) => gen.generate(),
                IdGeneratorType::Random => generate_random_id(7),
            };

            // Check for collision
            if !self.urls.contains_key(&code) {
                return Ok(code);
            }
        }

        Err("Failed to generate unique code")
    }

    /// Resolve a short code to the original URL
    pub fn resolve(&self, short_code: &str) -> Option<String> {
        // Check cache first
        if let Some(entry) = self.cache.get(short_code) {
            let (url, cached_at) = entry.value();
            if cached_at.elapsed() < self.cache_ttl {
                self.record_click(short_code);
                return Some(url.long_url.clone());
            }
        }

        // Cache miss - check database
        if let Some(url) = self.urls.get(short_code) {
            if url.is_expired() {
                return None;
            }

            // Update cache
            self.cache.insert(
                short_code.to_string(),
                (url.value().clone(), Instant::now()),
            );

            // Track click
            self.record_click(short_code);

            return Some(url.long_url.clone());
        }

        None
    }

    /// Record a click event (for analytics)
    fn record_click(&self, short_code: &str) {
        let event = ClickEvent {
            short_code: short_code.to_string(),
            timestamp: Utc::now(),
            user_agent: None,
            referrer: None,
            ip_address: None,
        };

        self.clicks
            .entry(short_code.to_string())
            .or_default()
            .push(event);
    }

    /// Get URL statistics
    pub fn get_stats(&self, short_code: &str) -> Option<UrlStats> {
        self.urls.get(short_code).map(|url| {
            let click_count = self
                .clicks
                .get(short_code)
                .map(|events| events.len() as u64)
                .unwrap_or(0);
            UrlStats {
                short_code: short_code.to_string(),
                long_url: url.long_url.clone(),
                click_count,
                created_at: url.created_at,
            }
        })
    }

    /// Get total URL count
    pub fn url_count(&self) -> usize {
        self.urls.len()
    }
}

#[derive(Debug, Serialize)]
pub struct UrlStats {
    pub short_code: String,
    pub long_url: String,
    pub click_count: u64,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// Demonstration
// =============================================================================

fn main() {
    println!("=== URL Shortener Demo ===\n");

    // Demo 1: Basic URL shortening with different strategies
    println!("\n  ═══ ID Generation Strategies ═══\n");

    // Hash-based
    let hash_id = generate_hash_id("https://example.com/very/long/url/path", 7);
    println!("Hash-based ID: {}", hash_id);

    // Same URL = same hash
    let hash_id2 = generate_hash_id("https://example.com/very/long/url/path", 7);
    println!(
        "Same URL hash: {} (deterministic: {})",
        hash_id2,
        hash_id == hash_id2
    );

    // Counter-based
    let counter_gen = CounterIdGenerator::new(1);
    println!("\nCounter-based IDs:");
    for _ in 0..5 {
        print!("{} ", counter_gen.generate());
    }
    println!();

    // Random
    println!("\nRandom IDs:");
    for _ in 0..5 {
        print!("{} ", generate_random_id(7));
    }
    println!();

    // Demo 2: Full URL Shortener
    println!("\n--- URL Shortener Service ---\n");
    let shortener = UrlShortener::new(IdGeneratorType::Random);

    // Shorten some URLs
    let urls = vec![
        "https://www.rust-lang.org/learn",
        "https://github.com/rust-lang/rust",
        "https://doc.rust-lang.org/book/",
        "https://crates.io/crates/tokio",
    ];

    println!("Shortening URLs:");
    for url in &urls {
        let code = shortener.shorten(url, None).unwrap();
        println!("  {} → {}", url, code);
    }

    // Custom alias
    let custom = shortener
        .shorten("https://mycompany.com/product", Some("myproduct"))
        .unwrap();
    println!("  Custom alias: myproduct → {}", custom);

    // Try duplicate custom alias
    let duplicate = shortener.shorten("https://other.com/stuff", Some("myproduct"));
    println!("  Duplicate alias: {:?}", duplicate);

    // Demo 3: Resolution and caching
    println!("\n--- URL Resolution ---\n");

    // Get a short code we created
    let code = shortener.shorten(urls[0], None).unwrap();

    // Resolve multiple times (shows caching behavior)
    println!("Resolving '{}' multiple times:", code);
    for i in 1..=5 {
        let resolved = shortener.resolve(&code);
        println!("  Attempt {}: {:?}", i, resolved.is_some());
    }

    // Demo 4: Analytics
    println!("\n--- Analytics ---\n");

    if let Some(stats) = shortener.get_stats(&code) {
        println!("Stats for '{}':", code);
        println!("  Original URL: {}", stats.long_url);
        println!("  Click count: {}", stats.click_count);
        println!("  Created at: {}", stats.created_at);
    }

    // Demo 5: Capacity estimation
    println!("\n--- Capacity Estimation ---\n");

    let daily_urls: u64 = 100_000_000;
    let read_write_ratio: u64 = 100;
    let years: u64 = 5;
    let url_size_bytes: u64 = 500;

    let writes_per_sec = daily_urls / 86400;
    let reads_per_sec = writes_per_sec * read_write_ratio;
    let total_urls = daily_urls * 365 * years;
    let storage_bytes = total_urls * url_size_bytes;

    println!("Daily URLs: {} million", daily_urls / 1_000_000);
    println!("Write/sec: ~{}", writes_per_sec);
    println!("Read/sec: ~{}", reads_per_sec);
    println!(
        "URLs over {} years: {} billion",
        years,
        total_urls / 1_000_000_000
    );
    println!("Storage needed: {} TB", storage_bytes / 1_000_000_000_000);

    // Base62 capacity
    let base62_7 = 62_u64.pow(7);
    println!(
        "\nBase62 capacity (7 chars): {} ({:.1} trillion)",
        base62_7,
        base62_7 as f64 / 1e12
    );

    println!("\n=== Demo Complete ===");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base62_encoding() {
        assert_eq!(encode_base62(0), "0");
        assert_eq!(encode_base62(61), "z");
        assert_eq!(encode_base62(62), "10");
    }

    #[test]
    fn test_hash_deterministic() {
        let url = "https://example.com";
        let hash1 = generate_hash_id(url, 7);
        let hash2 = generate_hash_id(url, 7);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_shorten_and_resolve() {
        let shortener = UrlShortener::new(IdGeneratorType::Random);
        let original = "https://example.com/test";

        let code = shortener.shorten(original, None).unwrap();
        let resolved = shortener.resolve(&code);

        assert_eq!(resolved, Some(original.to_string()));
    }

    #[test]
    fn test_custom_alias() {
        let shortener = UrlShortener::new(IdGeneratorType::Random);

        let code = shortener
            .shorten("https://example.com", Some("custom"))
            .unwrap();

        assert_eq!(code, "custom");

        // Duplicate should fail
        let result = shortener.shorten("https://other.com", Some("custom"));
        assert!(result.is_err());
    }

    #[test]
    fn test_click_tracking() {
        let shortener = UrlShortener::new(IdGeneratorType::Random);
        let code = shortener.shorten("https://example.com", None).unwrap();

        // Resolve 5 times
        for _ in 0..5 {
            shortener.resolve(&code);
        }

        let stats = shortener.get_stats(&code).unwrap();
        assert_eq!(stats.click_count, 5);
    }
}
