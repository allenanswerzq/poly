#![allow(dead_code, unused_variables, unused_imports)]
//! # Live Comments (FB Live Comments) - Mini Implementation
//!
//! Demonstrates:
//! - Real-time comment streaming
//! - Comment sampling for high-volume streams
//! - Spam detection basics
//! - Sentiment analysis simulation
//!
//! Run: cargo run -p live-comments

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

fn instant_now() -> Instant {
    Instant::now()
}

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Comment {
    id: String,
    stream_id: String,
    user_id: String,
    content: String,
    #[serde(skip, default = "instant_now")]
    timestamp: Instant,
    sentiment: Sentiment,
    is_spam: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Sentiment {
    Positive,
    Neutral,
    Negative,
}

#[derive(Debug, Clone)]
struct StreamStats {
    total_comments: u64,
    comments_per_second: f64,
    spam_blocked: u64,
    sentiment_distribution: HashMap<String, u64>,
}

// =============================================================================
// Spam Detector (Simple Rule-Based)
// =============================================================================

struct SpamDetector {
    blocked_phrases: Vec<String>,
    user_comment_counts: DashMap<String, AtomicU64>,
    rate_limit_per_second: u64,
}

impl SpamDetector {
    fn new() -> Self {
        Self {
            blocked_phrases: vec![
                "buy now".to_string(),
                "click here".to_string(),
                "free money".to_string(),
                "follow me".to_string(),
            ],
            user_comment_counts: DashMap::new(),
            rate_limit_per_second: 5,
        }
    }

    fn is_spam(&self, user_id: &str, content: &str) -> bool {
        // Check blocked phrases
        let lower = content.to_lowercase();
        for phrase in &self.blocked_phrases {
            if lower.contains(phrase) {
                return true;
            }
        }

        // Check rate limiting
        let count = self
            .user_comment_counts
            .entry(user_id.to_string())
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);

        count >= self.rate_limit_per_second
    }

    fn reset_counts(&self) {
        for entry in self.user_comment_counts.iter() {
            entry.value().store(0, Ordering::SeqCst);
        }
    }
}

// =============================================================================
// Sentiment Analyzer (Simplified)
// =============================================================================

struct SentimentAnalyzer {
    positive_words: Vec<&'static str>,
    negative_words: Vec<&'static str>,
}

impl SentimentAnalyzer {
    fn new() -> Self {
        Self {
            positive_words: vec![
                "love",
                "great",
                "awesome",
                "amazing",
                "best",
                "good",
                "nice",
                "❤️",
                "😊",
                "👍",
                "🔥",
                "💯",
                "wow",
                "incredible",
                "perfect",
            ],
            negative_words: vec![
                "hate",
                "bad",
                "worst",
                "terrible",
                "awful",
                "boring",
                "sucks",
                "😡",
                "👎",
                "💔",
                "😢",
                "ugh",
                "disappointed",
                "stupid",
            ],
        }
    }

    fn analyze(&self, content: &str) -> Sentiment {
        let lower = content.to_lowercase();

        let positive_count = self
            .positive_words
            .iter()
            .filter(|w| lower.contains(*w))
            .count();

        let negative_count = self
            .negative_words
            .iter()
            .filter(|w| lower.contains(*w))
            .count();

        if positive_count > negative_count {
            Sentiment::Positive
        } else if negative_count > positive_count {
            Sentiment::Negative
        } else {
            Sentiment::Neutral
        }
    }
}

// =============================================================================
// Comment Sampler (For High-Volume Streams)
// =============================================================================

struct CommentSampler {
    target_rate: f64, // Target comments per second
    reservoir_size: usize,
    reservoir: Mutex<Vec<Comment>>,
    current_count: AtomicU64,
}

impl CommentSampler {
    fn new(target_rate: f64, reservoir_size: usize) -> Self {
        Self {
            target_rate,
            reservoir_size,
            reservoir: Mutex::new(Vec::with_capacity(reservoir_size)),
            current_count: AtomicU64::new(0),
        }
    }

    fn add(&self, comment: Comment, current_rate: f64) {
        let n = self.current_count.fetch_add(1, Ordering::SeqCst) + 1;

        // Sampling probability based on desired rate
        let sample_prob = (self.target_rate / current_rate).min(1.0);
        let should_sample = rand::thread_rng().gen::<f64>() < sample_prob;

        if should_sample {
            let mut reservoir = self.reservoir.lock();

            if reservoir.len() < self.reservoir_size {
                reservoir.push(comment);
            } else {
                // Reservoir sampling
                let idx = rand::thread_rng().gen_range(0..n as usize);
                if idx < self.reservoir_size {
                    reservoir[idx] = comment;
                }
            }
        }
    }

    fn drain(&self) -> Vec<Comment> {
        let mut reservoir = self.reservoir.lock();
        std::mem::take(&mut *reservoir)
    }
}

// =============================================================================
// Live Stream
// =============================================================================

struct LiveStream {
    id: String,
    comments: RwLock<VecDeque<Comment>>,
    subscribers: DashMap<String, mpsc::UnboundedSender<Comment>>,
    spam_detector: SpamDetector,
    sentiment_analyzer: SentimentAnalyzer,
    sampler: CommentSampler,
    stats: StreamStatsTracker,
    comment_counter: AtomicU64,
    max_comments: usize,
}

struct StreamStatsTracker {
    total_comments: AtomicU64,
    spam_blocked: AtomicU64,
    positive: AtomicU64,
    negative: AtomicU64,
    neutral: AtomicU64,
    window_counts: Mutex<VecDeque<(Instant, u64)>>,
}

impl StreamStatsTracker {
    fn new() -> Self {
        Self {
            total_comments: AtomicU64::new(0),
            spam_blocked: AtomicU64::new(0),
            positive: AtomicU64::new(0),
            negative: AtomicU64::new(0),
            neutral: AtomicU64::new(0),
            window_counts: Mutex::new(VecDeque::new()),
        }
    }

    fn record_comment(&self, sentiment: Sentiment) {
        self.total_comments.fetch_add(1, Ordering::SeqCst);

        match sentiment {
            Sentiment::Positive => self.positive.fetch_add(1, Ordering::SeqCst),
            Sentiment::Negative => self.negative.fetch_add(1, Ordering::SeqCst),
            Sentiment::Neutral => self.neutral.fetch_add(1, Ordering::SeqCst),
        };

        let mut window = self.window_counts.lock();
        window.push_back((Instant::now(), 1));
    }

    fn record_spam(&self) {
        self.spam_blocked.fetch_add(1, Ordering::SeqCst);
    }

    fn get_rate(&self) -> f64 {
        let mut window = self.window_counts.lock();
        let now = Instant::now();

        // Remove old entries (older than 1 second)
        while let Some((ts, _)) = window.front() {
            if now.duration_since(*ts) > Duration::from_secs(1) {
                window.pop_front();
            } else {
                break;
            }
        }

        window.len() as f64
    }

    fn get_stats(&self) -> StreamStats {
        StreamStats {
            total_comments: self.total_comments.load(Ordering::SeqCst),
            comments_per_second: self.get_rate(),
            spam_blocked: self.spam_blocked.load(Ordering::SeqCst),
            sentiment_distribution: HashMap::from([
                ("positive".to_string(), self.positive.load(Ordering::SeqCst)),
                ("neutral".to_string(), self.neutral.load(Ordering::SeqCst)),
                ("negative".to_string(), self.negative.load(Ordering::SeqCst)),
            ]),
        }
    }
}

impl LiveStream {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            comments: RwLock::new(VecDeque::new()),
            subscribers: DashMap::new(),
            spam_detector: SpamDetector::new(),
            sentiment_analyzer: SentimentAnalyzer::new(),
            sampler: CommentSampler::new(10.0, 100), // 10 comments/sec target
            stats: StreamStatsTracker::new(),
            comment_counter: AtomicU64::new(0),
            max_comments: 1000, // Keep last 1000 comments
        }
    }

    fn post_comment(&self, user_id: &str, content: &str) -> Result<Comment, &'static str> {
        // Check for spam
        if self.spam_detector.is_spam(user_id, content) {
            self.stats.record_spam();
            return Err("Comment blocked as spam");
        }

        // Analyze sentiment
        let sentiment = self.sentiment_analyzer.analyze(content);

        let comment = Comment {
            id: format!(
                "cmt_{}",
                self.comment_counter.fetch_add(1, Ordering::SeqCst)
            ),
            stream_id: self.id.clone(),
            user_id: user_id.to_string(),
            content: content.to_string(),
            timestamp: Instant::now(),
            sentiment,
            is_spam: false,
        };

        // Store comment
        {
            let mut comments = self.comments.write();
            comments.push_back(comment.clone());
            while comments.len() > self.max_comments {
                comments.pop_front();
            }
        }

        // Update stats
        self.stats.record_comment(sentiment);

        // Add to sampler
        let rate = self.stats.get_rate();
        self.sampler.add(comment.clone(), rate);

        // Broadcast to subscribers
        for entry in self.subscribers.iter() {
            let _ = entry.value().send(comment.clone());
        }

        Ok(comment)
    }

    fn subscribe(&self, subscriber_id: &str) -> mpsc::UnboundedReceiver<Comment> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.insert(subscriber_id.to_string(), tx);
        rx
    }

    fn get_recent(&self, limit: usize) -> Vec<Comment> {
        let comments = self.comments.read();
        comments.iter().rev().take(limit).cloned().collect()
    }

    fn get_sampled(&self) -> Vec<Comment> {
        self.sampler.drain()
    }

    fn get_stats(&self) -> StreamStats {
        self.stats.get_stats()
    }
}

// =============================================================================
// Live Comments Service
// =============================================================================

struct LiveCommentsService {
    streams: DashMap<String, Arc<LiveStream>>,
}

impl LiveCommentsService {
    fn new() -> Self {
        Self {
            streams: DashMap::new(),
        }
    }

    fn create_stream(&self, stream_id: &str) -> Arc<LiveStream> {
        let stream = Arc::new(LiveStream::new(stream_id));
        self.streams
            .insert(stream_id.to_string(), Arc::clone(&stream));
        stream
    }

    fn get_stream(&self, stream_id: &str) -> Option<Arc<LiveStream>> {
        self.streams.get(stream_id).map(|s| Arc::clone(&s))
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Live Comments (FB Live) Demo ===\n");

    let service = LiveCommentsService::new();
    let stream = service.create_stream("stream_001");

    // Simulate comments
    println!("\n  ═══ Posting Comments ═══");

    let comments = vec![
        ("user1", "This is awesome! 🔥"),
        ("user2", "Love this stream! ❤️"),
        ("user3", "Meh, kind of boring"),
        ("user4", "Amazing content! 👍"),
        ("user5", "I hate this so much 😡"),
        ("spammer", "BUY NOW at mysite.com!"),
        ("user6", "Great explanation!"),
        ("user7", "Worst stream ever"),
        ("user8", "Perfect! 💯"),
    ];

    for (user, content) in &comments {
        match stream.post_comment(user, content) {
            Ok(cmt) => {
                println!(
                    "  {} ({}): \"{}\" [{:?}]",
                    cmt.user_id, cmt.id, content, cmt.sentiment
                );
            }
            Err(e) => {
                println!("  {} ❌: \"{}\" - {}", user, content, e);
            }
        }
    }

    // Show stats
    println!("\n--- Stream Stats ---");
    let stats = stream.get_stats();
    println!("Total comments: {}", stats.total_comments);
    println!("Spam blocked: {}", stats.spam_blocked);
    println!("Sentiment distribution:");
    for (sentiment, count) in &stats.sentiment_distribution {
        let pct = if stats.total_comments > 0 {
            count * 100 / stats.total_comments
        } else {
            0
        };
        println!("  {}: {} ({}%)", sentiment, count, pct);
    }

    // Recent comments
    println!("\n--- Recent Comments ---");
    let recent = stream.get_recent(5);
    println!("Last 5 comments:");
    for cmt in recent {
        println!("  @{}: {}", cmt.user_id, cmt.content);
    }

    // Sampled comments (for high volume)
    println!("\n--- Sampled Comments ---");
    let sampled = stream.get_sampled();
    println!("{} sampled comments (reservoir sampling)", sampled.len());

    // Simulate high-volume stream
    println!("\n--- High Volume Simulation ---");
    let high_volume_stream = service.create_stream("viral_stream");

    for i in 0..100 {
        let user = format!("user_{}", i % 20);
        let content = if i % 10 == 0 {
            "This is amazing! 🔥".to_string()
        } else if i % 7 == 0 {
            "Not great...".to_string()
        } else {
            format!("Comment #{}", i)
        };

        let _ = high_volume_stream.post_comment(&user, &content);
    }

    let high_stats = high_volume_stream.get_stats();
    println!("High volume stream:");
    println!("  Total: {} comments", high_stats.total_comments);
    println!("  Rate: {:.1} comments/sec", high_stats.comments_per_second);
    println!(
        "  Sampled: {} comments",
        high_volume_stream.get_sampled().len()
    );

    println!("\n=== Key Concepts ===");
    println!("1. Real-time: Broadcast comments to all subscribers");
    println!("2. Spam Detection: Block suspicious content + rate limit");
    println!("3. Sentiment Analysis: Classify positive/negative/neutral");
    println!("4. Sampling: Reservoir sampling for high-volume streams");
    println!("5. Windowed Stats: Track comments/second with sliding window");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spam_detection() {
        let detector = SpamDetector::new();

        assert!(detector.is_spam("user1", "BUY NOW click here!"));
        assert!(!detector.is_spam("user1", "Great stream!"));
    }

    #[test]
    fn test_sentiment_analysis() {
        let analyzer = SentimentAnalyzer::new();

        assert!(matches!(
            analyzer.analyze("I love this!"),
            Sentiment::Positive
        ));
        assert!(matches!(
            analyzer.analyze("This is terrible"),
            Sentiment::Negative
        ));
        assert!(matches!(analyzer.analyze("Hello"), Sentiment::Neutral));
    }

    #[test]
    fn test_comment_posting() {
        let stream = LiveStream::new("test");

        let result = stream.post_comment("user1", "Great!");
        assert!(result.is_ok());

        let recent = stream.get_recent(10);
        assert_eq!(recent.len(), 1);
    }
}
