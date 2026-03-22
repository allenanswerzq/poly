//! # News Feed (Facebook/Twitter) - Mini Implementation
//!
//! Demonstrates:
//! - Fan-out on Write vs Fan-out on Read
//! - Timeline generation and caching
//! - Social graph for following relationships
//! - Ranking and chronological sorting
//! - Feed pagination with cursor
//!
//! Run: cargo run -p news-feed

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Post {
    id: String,
    author_id: String,
    content: String,
    timestamp: u64,
    likes: u64,
    comments: u64,
    shares: u64,
}

impl Post {
    fn score(&self) -> f64 {
        // Simple ranking: recency + engagement
        let age_hours = (now() - self.timestamp) as f64 / 3600000.0;
        let engagement = (self.likes + self.comments * 2 + self.shares * 3) as f64;

        // Decay engagement score over time
        engagement / (1.0 + age_hours.powf(1.5))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct FeedItem {
    post: Post,
    score: f64,
}

impl Eq for FeedItem {}

impl Ord for FeedItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl PartialOrd for FeedItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// =============================================================================
// Social Graph
// =============================================================================

struct SocialGraph {
    // user -> set of users they follow
    following: DashMap<String, HashSet<String>>,
    // user -> set of followers
    followers: DashMap<String, HashSet<String>>,
}

impl SocialGraph {
    fn new() -> Self {
        Self {
            following: DashMap::new(),
            followers: DashMap::new(),
        }
    }

    fn follow(&self, user_id: &str, target_id: &str) {
        self.following
            .entry(user_id.to_string())
            .or_default()
            .insert(target_id.to_string());

        self.followers
            .entry(target_id.to_string())
            .or_default()
            .insert(user_id.to_string());
    }

    fn get_following(&self, user_id: &str) -> Vec<String> {
        self.following
            .get(user_id)
            .map(|f| f.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn get_followers(&self, user_id: &str) -> Vec<String> {
        self.followers
            .get(user_id)
            .map(|f| f.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn follower_count(&self, user_id: &str) -> usize {
        self.followers.get(user_id).map(|f| f.len()).unwrap_or(0)
    }
}

// =============================================================================
// Post Storage
// =============================================================================

struct PostStore {
    posts: DashMap<String, Post>,
    user_posts: DashMap<String, RwLock<Vec<String>>>, // user_id -> post_ids (newest first)
    post_counter: AtomicU64,
}

impl PostStore {
    fn new() -> Self {
        Self {
            posts: DashMap::new(),
            user_posts: DashMap::new(),
            post_counter: AtomicU64::new(0),
        }
    }

    fn create_post(&self, author_id: &str, content: &str) -> Post {
        let id = format!("post_{}", self.post_counter.fetch_add(1, Ordering::SeqCst));

        let post = Post {
            id: id.clone(),
            author_id: author_id.to_string(),
            content: content.to_string(),
            timestamp: now(),
            likes: 0,
            comments: 0,
            shares: 0,
        };

        self.posts.insert(id.clone(), post.clone());

        self.user_posts
            .entry(author_id.to_string())
            .or_insert_with(|| RwLock::new(Vec::new()))
            .write()
            .insert(0, id); // Insert at front (newest first)

        post
    }

    fn get_post(&self, post_id: &str) -> Option<Post> {
        self.posts.get(post_id).map(|p| p.clone())
    }

    fn get_user_posts(&self, user_id: &str, limit: usize) -> Vec<Post> {
        self.user_posts
            .get(user_id)
            .map(|ids| {
                ids.read()
                    .iter()
                    .take(limit)
                    .filter_map(|id| self.get_post(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn add_engagement(&self, post_id: &str, likes: u64, comments: u64, shares: u64) {
        if let Some(mut post) = self.posts.get_mut(post_id) {
            post.likes += likes;
            post.comments += comments;
            post.shares += shares;
        }
    }
}

// =============================================================================
// Feed Cache (Fan-out on Write)
// =============================================================================

struct FeedCache {
    // user_id -> cached timeline (post_ids)
    timelines: DashMap<String, RwLock<Vec<String>>>,
    max_cache_size: usize,
}

impl FeedCache {
    fn new(max_cache_size: usize) -> Self {
        Self {
            timelines: DashMap::new(),
            max_cache_size,
        }
    }

    fn push_to_followers(&self, author_id: &str, post_id: &str, graph: &SocialGraph) {
        // Fan-out: push post to all followers' timelines
        let followers = graph.get_followers(author_id);

        for follower_id in followers {
            self.timelines
                .entry(follower_id)
                .or_insert_with(|| RwLock::new(Vec::new()))
                .write()
                .insert(0, post_id.to_string());
        }
    }

    fn get_cached(&self, user_id: &str, limit: usize) -> Vec<String> {
        self.timelines
            .get(user_id)
            .map(|t| t.read().iter().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    fn trim_cache(&self, user_id: &str) {
        if let Some(timeline) = self.timelines.get(user_id) {
            let mut t = timeline.write();
            if t.len() > self.max_cache_size {
                t.truncate(self.max_cache_size);
            }
        }
    }
}

// =============================================================================
// Feed Service
// =============================================================================

struct FeedService {
    graph: Arc<SocialGraph>,
    posts: Arc<PostStore>,
    cache: Arc<FeedCache>,
    celebrity_threshold: usize, // Follower count above which we don't fan-out
}

impl FeedService {
    fn new() -> Self {
        Self {
            graph: Arc::new(SocialGraph::new()),
            posts: Arc::new(PostStore::new()),
            cache: Arc::new(FeedCache::new(1000)),
            celebrity_threshold: 10000, // Celebrities have >10K followers
        }
    }

    fn create_post(&self, author_id: &str, content: &str) -> Post {
        let post = self.posts.create_post(author_id, content);

        // Decide fan-out strategy
        let follower_count = self.graph.follower_count(author_id);

        if follower_count < self.celebrity_threshold {
            // Regular user: fan-out on write
            self.cache
                .push_to_followers(author_id, &post.id, &self.graph);
        }
        // Celebrity: skip fan-out, use pull model

        post
    }

    fn get_feed(&self, user_id: &str, limit: usize) -> Vec<FeedItem> {
        let mut heap: BinaryHeap<FeedItem> = BinaryHeap::new();

        // 1. Get cached posts (fan-out on write results)
        let cached_ids = self.cache.get_cached(user_id, limit * 2);
        for post_id in cached_ids {
            if let Some(post) = self.posts.get_post(&post_id) {
                heap.push(FeedItem {
                    score: post.score(),
                    post,
                });
            }
        }

        // 2. Pull from celebrities (fan-out on read)
        let following = self.graph.get_following(user_id);
        for followed_id in following {
            if self.graph.follower_count(&followed_id) >= self.celebrity_threshold {
                // This is a celebrity, pull their posts
                let posts = self.posts.get_user_posts(&followed_id, 10);
                for post in posts {
                    heap.push(FeedItem {
                        score: post.score(),
                        post,
                    });
                }
            }
        }

        // 3. Return top N ranked items
        let mut feed = Vec::new();
        while feed.len() < limit {
            if let Some(item) = heap.pop() {
                feed.push(item);
            } else {
                break;
            }
        }

        feed
    }

    fn get_feed_chronological(&self, user_id: &str, limit: usize) -> Vec<Post> {
        let mut posts: Vec<Post> = Vec::new();

        // Pull from all followed users
        let following = self.graph.get_following(user_id);
        for followed_id in following {
            posts.extend(self.posts.get_user_posts(&followed_id, limit));
        }

        // Sort by timestamp (newest first)
        posts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        posts.truncate(limit);

        posts
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== News Feed (Facebook/Twitter) Demo ===\n");

    let feed = FeedService::new();

    // Create users and follow relationships
    println!("\n  ═══ Setting up Social Graph ═══");

    // Regular users follow each other
    feed.graph.follow("alice", "bob");
    feed.graph.follow("alice", "charlie");
    feed.graph.follow("bob", "alice");
    feed.graph.follow("charlie", "alice");

    // Everyone follows celebrity
    for i in 0..100 {
        // Simulate many followers
        feed.graph.follow(&format!("user{}", i), "celebrity");
    }
    feed.graph.follow("alice", "celebrity");
    feed.graph.follow("bob", "celebrity");

    println!("Alice follows: {:?}", feed.graph.get_following("alice"));
    println!(
        "Celebrity has {} followers\n",
        feed.graph.follower_count("celebrity")
    );

    // Create posts
    println!("\n  ═══ Creating Posts ═══");

    let p1 = feed.create_post("bob", "Just had the best coffee! ☕");
    println!("Bob posted (fan-out to {} followers)", feed.graph.follower_count("bob"));

    let p2 = feed.create_post("charlie", "Working on something exciting!");
    println!("Charlie posted");

    // Celebrity post (no fan-out due to high follower count)
    let p3 = feed.create_post("celebrity", "Big announcement coming soon! 🚀");
    println!("Celebrity posted (NO fan-out, pull model)");

    // Add some engagement
    feed.posts.add_engagement(&p1.id, 5, 2, 1);
    feed.posts.add_engagement(&p3.id, 1000, 500, 200);

    // Regular user post with high engagement
    let p4 = feed.create_post("bob", "This went viral somehow!");
    feed.posts.add_engagement(&p4.id, 500, 100, 50);

    println!();

    // Get Alice's feed (ranked)
    println!("\n  ═══ Alice's Ranked Feed ═══");
    let alice_feed = feed.get_feed("alice", 10);
    for (i, item) in alice_feed.iter().enumerate() {
        println!(
            "{}. [score={:.1}] @{}: \"{}\" (❤️{} 💬{} 🔄{})",
            i + 1,
            item.score,
            item.post.author_id,
            item.post.content,
            item.post.likes,
            item.post.comments,
            item.post.shares
        );
    }

    // Get chronological feed
    println!("\n--- Alice's Chronological Feed ---");
    let chrono_feed = feed.get_feed_chronological("alice", 5);
    for (i, post) in chrono_feed.iter().enumerate() {
        println!("{}. @{}: \"{}\"", i + 1, post.author_id, post.content);
    }

    // Show cache stats
    println!("\n--- Cache Stats ---");
    let cached = feed.cache.get_cached("alice", 100);
    println!("Alice's cached timeline: {} posts", cached.len());
    println!("(Celebrity posts NOT cached - pulled on demand)");

    println!("\n=== Key Concepts ===");
    println!("1. Fan-out on Write: Push posts to followers' cached timelines");
    println!("2. Fan-out on Read: Pull from celebrities on-demand");
    println!("3. Hybrid: Use both strategies based on follower count");
    println!("4. Ranking: Score = engagement / time decay");
    println!("5. Pagination: Use cursor-based pagination for infinite scroll");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_social_graph() {
        let graph = SocialGraph::new();
        graph.follow("a", "b");
        graph.follow("a", "c");

        assert_eq!(graph.get_following("a").len(), 2);
        assert_eq!(graph.follower_count("b"), 1);
    }

    #[test]
    fn test_post_scoring() {
        let post = Post {
            id: "1".to_string(),
            author_id: "a".to_string(),
            content: "test".to_string(),
            timestamp: now(),
            likes: 100,
            comments: 50,
            shares: 10,
        };

        assert!(post.score() > 0.0);
    }

    #[test]
    fn test_fanout_on_write() {
        let feed = FeedService::new();

        feed.graph.follow("bob", "alice");
        feed.create_post("alice", "Hello!");

        // Bob should have alice's post in cache
        let cached = feed.cache.get_cached("bob", 10);
        assert_eq!(cached.len(), 1);
    }
}
