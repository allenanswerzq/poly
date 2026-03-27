#![allow(dead_code, unused_variables, unused_imports)]
//! # Web Crawler - Mini Implementation
//!
//! A distributed web crawler demonstrating:
//! - URL Frontier with priority queue
//! - Politeness (robots.txt, rate limiting per domain)
//! - URL deduplication with bloom filter
//! - Multi-threaded crawling
//! - Content extraction and storage
//!
//! Run: cargo run -p web-crawler

use dashmap::DashMap;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{BinaryHeap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

// =============================================================================
// URL Frontier - Priority Queue with Politeness
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
struct CrawlUrl {
    url: String,
    domain: String,
    depth: u32,
    priority: u32, // Higher = more important
    discovered_at: u64,
}

impl Ord for CrawlUrl {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then older (lower discovered_at) first
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.discovered_at.cmp(&self.discovered_at))
    }
}

impl PartialOrd for CrawlUrl {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct UrlFrontier {
    // Per-domain queues for politeness
    domain_queues: DashMap<String, VecDeque<CrawlUrl>>,
    // Global priority queue pointing to domains
    priority_queue: Mutex<BinaryHeap<(u32, String)>>, // (priority, domain)
    // Track last crawl time per domain
    last_crawl: DashMap<String, Instant>,
    // Min delay between requests to same domain
    politeness_delay: Duration,
    total_queued: AtomicUsize,
}

impl UrlFrontier {
    fn new(politeness_delay: Duration) -> Self {
        Self {
            domain_queues: DashMap::new(),
            priority_queue: Mutex::new(BinaryHeap::new()),
            last_crawl: DashMap::new(),
            politeness_delay,
            total_queued: AtomicUsize::new(0),
        }
    }

    fn add(&self, url: CrawlUrl) {
        let domain = url.domain.clone();
        let priority = url.priority;

        let mut queue = self.domain_queues.entry(domain.clone()).or_default();
        let was_empty = queue.is_empty();
        queue.push_back(url);

        if was_empty {
            self.priority_queue.lock().push((priority, domain));
        }

        self.total_queued.fetch_add(1, Ordering::SeqCst);
    }

    fn get_next(&self) -> Option<CrawlUrl> {
        let mut pq = self.priority_queue.lock();

        // Find a domain that's ready to crawl
        let mut skipped = Vec::new();

        while let Some((priority, domain)) = pq.pop() {
            // Check politeness delay
            let can_crawl = self
                .last_crawl
                .get(&domain)
                .map(|t| t.elapsed() >= self.politeness_delay)
                .unwrap_or(true);

            if can_crawl {
                // Get URL from domain queue
                if let Some(mut queue) = self.domain_queues.get_mut(&domain) {
                    if let Some(url) = queue.pop_front() {
                        // Re-add domain to priority queue if more URLs
                        if !queue.is_empty() {
                            pq.push((priority, domain.clone()));
                        }

                        // Update last crawl time
                        self.last_crawl.insert(domain, Instant::now());
                        self.total_queued.fetch_sub(1, Ordering::SeqCst);

                        // Put back skipped domains
                        for s in skipped {
                            pq.push(s);
                        }

                        return Some(url);
                    }
                }
            } else {
                skipped.push((priority, domain));
            }
        }

        // Put back skipped domains
        for s in skipped {
            pq.push(s);
        }

        None
    }

    fn size(&self) -> usize {
        self.total_queued.load(Ordering::SeqCst)
    }
}

// =============================================================================
// URL Deduplication - Bloom Filter
// =============================================================================

struct BloomFilter {
    bits: Vec<AtomicU64>,
    num_bits: usize,
    num_hashes: usize,
}

impl BloomFilter {
    fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        // Calculate optimal size
        let num_bits = (-(expected_items as f64 * false_positive_rate.ln())
            / (2.0_f64.ln().powi(2)))
        .ceil() as usize;
        let num_hashes = ((num_bits as f64 / expected_items as f64) * 2.0_f64.ln()).ceil() as usize;

        let num_words = num_bits.div_ceil(64);
        let bits = (0..num_words).map(|_| AtomicU64::new(0)).collect();

        Self {
            bits,
            num_bits,
            num_hashes,
        }
    }

    fn hash(&self, url: &str, seed: usize) -> usize {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        hasher.update(seed.to_le_bytes());
        let result = hasher.finalize();

        let h = u64::from_le_bytes(result[0..8].try_into().unwrap());
        (h as usize) % self.num_bits
    }

    fn add(&self, url: &str) {
        for i in 0..self.num_hashes {
            let pos = self.hash(url, i);
            let word_idx = pos / 64;
            let bit_idx = pos % 64;
            self.bits[word_idx].fetch_or(1 << bit_idx, Ordering::SeqCst);
        }
    }

    fn might_contain(&self, url: &str) -> bool {
        for i in 0..self.num_hashes {
            let pos = self.hash(url, i);
            let word_idx = pos / 64;
            let bit_idx = pos % 64;
            if self.bits[word_idx].load(Ordering::SeqCst) & (1 << bit_idx) == 0 {
                return false;
            }
        }
        true
    }
}

// =============================================================================
// Robots.txt Parser (Simplified)
// =============================================================================

struct RobotsChecker {
    rules: DashMap<String, Vec<String>>, // domain -> disallowed paths
    crawl_delay: DashMap<String, Duration>,
}

impl RobotsChecker {
    fn new() -> Self {
        Self {
            rules: DashMap::new(),
            crawl_delay: DashMap::new(),
        }
    }

    fn add_rule(&self, domain: &str, disallow_path: &str) {
        self.rules
            .entry(domain.to_string())
            .or_default()
            .push(disallow_path.to_string());
    }

    fn set_crawl_delay(&self, domain: &str, delay: Duration) {
        self.crawl_delay.insert(domain.to_string(), delay);
    }

    fn is_allowed(&self, domain: &str, path: &str) -> bool {
        if let Some(disallowed) = self.rules.get(domain) {
            for pattern in disallowed.iter() {
                if path.starts_with(pattern) {
                    return false;
                }
            }
        }
        true
    }

    fn get_delay(&self, domain: &str) -> Duration {
        self.crawl_delay
            .get(domain)
            .map(|d| *d)
            .unwrap_or(Duration::from_millis(100))
    }
}

// =============================================================================
// Content Storage
// =============================================================================

#[derive(Debug, Clone)]
struct CrawledPage {
    url: String,
    content_hash: String,
    links: Vec<String>,
    crawled_at: Instant,
    depth: u32,
}

struct ContentStore {
    pages: DashMap<String, CrawledPage>,
    content_hashes: DashMap<String, String>, // hash -> url (for dedup)
}

impl ContentStore {
    fn new() -> Self {
        Self {
            pages: DashMap::new(),
            content_hashes: DashMap::new(),
        }
    }

    fn store(&self, page: CrawledPage) -> bool {
        // Check for duplicate content
        if self.content_hashes.contains_key(&page.content_hash) {
            return false; // Duplicate content
        }

        self.content_hashes
            .insert(page.content_hash.clone(), page.url.clone());
        self.pages.insert(page.url.clone(), page);
        true
    }

    fn count(&self) -> usize {
        self.pages.len()
    }
}

// =============================================================================
// Web Crawler
// =============================================================================

struct WebCrawler {
    frontier: Arc<UrlFrontier>,
    seen_urls: Arc<BloomFilter>,
    robots: Arc<RobotsChecker>,
    store: Arc<ContentStore>,
    max_depth: u32,
    stats: CrawlerStats,
}

struct CrawlerStats {
    pages_crawled: AtomicU64,
    urls_discovered: AtomicU64,
    duplicates_skipped: AtomicU64,
    robots_blocked: AtomicU64,
}

impl WebCrawler {
    fn new(max_depth: u32) -> Self {
        Self {
            frontier: Arc::new(UrlFrontier::new(Duration::from_millis(100))),
            seen_urls: Arc::new(BloomFilter::new(1_000_000, 0.01)),
            robots: Arc::new(RobotsChecker::new()),
            store: Arc::new(ContentStore::new()),
            max_depth,
            stats: CrawlerStats {
                pages_crawled: AtomicU64::new(0),
                urls_discovered: AtomicU64::new(0),
                duplicates_skipped: AtomicU64::new(0),
                robots_blocked: AtomicU64::new(0),
            },
        }
    }

    fn add_seed(&self, url: &str) {
        let domain = extract_domain(url);
        self.seen_urls.add(url);
        self.frontier.add(CrawlUrl {
            url: url.to_string(),
            domain,
            depth: 0,
            priority: 100,
            discovered_at: 0,
        });
    }

    async fn crawl_page(&self, crawl_url: &CrawlUrl) -> Option<CrawledPage> {
        // Check robots.txt
        let path = extract_path(&crawl_url.url);
        if !self.robots.is_allowed(&crawl_url.domain, &path) {
            self.stats.robots_blocked.fetch_add(1, Ordering::SeqCst);
            return None;
        }

        // Simulate fetching the page
        let (content, links) = simulate_fetch(&crawl_url.url).await;

        // Hash content
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        self.stats.pages_crawled.fetch_add(1, Ordering::SeqCst);

        Some(CrawledPage {
            url: crawl_url.url.clone(),
            content_hash,
            links,
            crawled_at: Instant::now(),
            depth: crawl_url.depth,
        })
    }

    fn process_links(&self, page: &CrawledPage, counter: &AtomicU64) {
        if page.depth >= self.max_depth {
            return;
        }

        for link in &page.links {
            // Check if already seen
            if self.seen_urls.might_contain(link) {
                self.stats.duplicates_skipped.fetch_add(1, Ordering::SeqCst);
                continue;
            }

            // Mark as seen and add to frontier
            self.seen_urls.add(link);
            let domain = extract_domain(link);

            self.frontier.add(CrawlUrl {
                url: link.clone(),
                domain,
                depth: page.depth + 1,
                priority: 100 - page.depth * 10, // Lower priority for deeper pages
                discovered_at: counter.fetch_add(1, Ordering::SeqCst),
            });

            self.stats.urls_discovered.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn print_stats(&self) {
        println!("Crawler Stats:");
        println!(
            "  Pages crawled: {}",
            self.stats.pages_crawled.load(Ordering::SeqCst)
        );
        println!(
            "  URLs discovered: {}",
            self.stats.urls_discovered.load(Ordering::SeqCst)
        );
        println!(
            "  Duplicates skipped: {}",
            self.stats.duplicates_skipped.load(Ordering::SeqCst)
        );
        println!(
            "  Robots blocked: {}",
            self.stats.robots_blocked.load(Ordering::SeqCst)
        );
        println!("  Unique pages stored: {}", self.store.count());
        println!("  Frontier size: {}", self.frontier.size());
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn extract_domain(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("unknown")
        .to_string()
}

fn extract_path(url: &str) -> String {
    let without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    if let Some(pos) = without_scheme.find('/') {
        without_scheme[pos..].to_string()
    } else {
        "/".to_string()
    }
}

async fn simulate_fetch(url: &str) -> (String, Vec<String>) {
    // Simulate network delay
    sleep(Duration::from_millis(10)).await;

    // Generate fake content and links
    let content = format!("Content of {}", url);
    let domain = extract_domain(url);

    // Generate 2-5 outgoing links
    let num_links = (url.len() % 4) + 2;
    let links: Vec<String> = (0..num_links)
        .map(|i| format!("https://{}/page{}{}", domain, url.len(), i))
        .collect();

    (content, links)
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() {
    println!("=== Web Crawler Demo ===\n");

    let crawler = Arc::new(WebCrawler::new(3)); // Max depth 3

    // Add robots.txt rules
    crawler.robots.add_rule("example.com", "/private/");
    crawler.robots.add_rule("example.com", "/admin/");
    crawler
        .robots
        .set_crawl_delay("example.com", Duration::from_millis(50));

    // Add seed URLs
    println!("Adding seed URLs...");
    crawler.add_seed("https://example.com/");
    crawler.add_seed("https://example.com/about");
    crawler.add_seed("https://test.com/");

    let counter = Arc::new(AtomicU64::new(1));

    // Crawl with multiple workers
    println!("Starting crawl with 4 workers...\n");

    let mut handles = vec![];

    for worker_id in 0..4 {
        let crawler = Arc::clone(&crawler);
        let counter = Arc::clone(&counter);

        handles.push(tokio::spawn(async move {
            let mut crawled = 0;

            for _ in 0..25 {
                // Each worker processes up to 25 URLs
                if let Some(url) = crawler.frontier.get_next() {
                    if let Some(page) = crawler.crawl_page(&url).await {
                        // Store page
                        crawler.store.store(page.clone());

                        // Process discovered links
                        crawler.process_links(&page, &counter);

                        crawled += 1;
                    }
                } else {
                    // No URL available, wait a bit
                    sleep(Duration::from_millis(10)).await;
                }
            }

            println!("Worker {} finished, crawled {} pages", worker_id, crawled);
        }));
    }

    // Wait for all workers
    for handle in handles {
        handle.await.unwrap();
    }

    println!();
    crawler.print_stats();

    // Show some crawled pages
    println!("\nSample crawled pages:");
    for (i, entry) in crawler.store.pages.iter().take(5).enumerate() {
        println!(
            "  {}. {} (depth={}, links={})",
            i + 1,
            entry.url,
            entry.depth,
            entry.links.len()
        );
    }

    println!("\n=== Key Concepts ===");
    println!("1. URL Frontier: Priority queue with per-domain politeness");
    println!("2. Deduplication: Bloom filter for seen URLs");
    println!("3. Robots.txt: Respect crawl rules and delays");
    println!("4. Content dedup: Hash content to avoid storing duplicates");
    println!("5. Multi-worker: Parallel crawling with coordination");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter() {
        let bf = BloomFilter::new(1000, 0.01);

        bf.add("https://example.com/page1");
        bf.add("https://example.com/page2");

        assert!(bf.might_contain("https://example.com/page1"));
        assert!(bf.might_contain("https://example.com/page2"));
        // Might have false positives, but very unlikely for unseen URLs
    }

    #[test]
    fn test_robots_checker() {
        let robots = RobotsChecker::new();
        robots.add_rule("example.com", "/private/");

        assert!(robots.is_allowed("example.com", "/public/page"));
        assert!(!robots.is_allowed("example.com", "/private/secret"));
    }

    #[test]
    fn test_url_frontier_politeness() {
        let frontier = UrlFrontier::new(Duration::from_millis(100));

        frontier.add(CrawlUrl {
            url: "https://a.com/1".to_string(),
            domain: "a.com".to_string(),
            depth: 0,
            priority: 100,
            discovered_at: 0,
        });

        frontier.add(CrawlUrl {
            url: "https://a.com/2".to_string(),
            domain: "a.com".to_string(),
            depth: 0,
            priority: 100,
            discovered_at: 1,
        });

        // First fetch should work
        assert!(frontier.get_next().is_some());

        // Second fetch from same domain should be blocked (politeness)
        // because we haven't waited long enough
        assert!(frontier.get_next().is_none());
    }
}
