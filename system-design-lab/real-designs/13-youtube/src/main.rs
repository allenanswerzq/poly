//! # YouTube (Video Platform) - Mini Implementation
//!
//! Demonstrates:
//! - Video upload and transcoding pipeline
//! - CDN simulation for video delivery
//! - Adaptive bitrate streaming
//! - View counting with approximate counters
//! - Recommendation engine basics
//!
//! Run: cargo run -p youtube

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone)]
struct Video {
    id: String,
    title: String,
    description: String,
    uploader_id: String,
    duration_secs: u64,
    upload_time: Instant,
    status: VideoStatus,
    transcoding_progress: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VideoStatus {
    Uploading,
    Transcoding,
    Ready,
    Failed,
}

#[derive(Debug, Clone)]
struct VideoVariant {
    video_id: String,
    resolution: Resolution,
    bitrate_kbps: u32,
    // Simulated chunked storage
    chunks: Vec<String>, // chunk IDs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Resolution {
    R360p,
    R480p,
    R720p,
    R1080p,
    R4K,
}

impl Resolution {
    fn bitrate(&self) -> u32 {
        match self {
            Resolution::R360p => 500,
            Resolution::R480p => 1000,
            Resolution::R720p => 2500,
            Resolution::R1080p => 5000,
            Resolution::R4K => 15000,
        }
    }
}

// =============================================================================
// Transcoding Pipeline
// =============================================================================

struct TranscodingPipeline {
    queue: Mutex<VecDeque<String>>, // video_ids waiting to transcode
    in_progress: DashMap<String, u8>, // video_id -> progress %
}

impl TranscodingPipeline {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            in_progress: DashMap::new(),
        }
    }

    fn submit(&self, video_id: &str) {
        self.queue.lock().push_back(video_id.to_string());
    }

    fn process_batch(&self, videos: &DashMap<String, Video>) -> Vec<String> {
        let mut completed = Vec::new();
        let mut queue = self.queue.lock();

        // Process up to 3 videos at a time
        while self.in_progress.len() < 3 {
            if let Some(video_id) = queue.pop_front() {
                self.in_progress.insert(video_id.clone(), 0);

                if let Some(mut video) = videos.get_mut(&video_id) {
                    video.status = VideoStatus::Transcoding;
                }
            } else {
                break;
            }
        }

        // Simulate progress
        let mut done = Vec::new();
        for mut entry in self.in_progress.iter_mut() {
            *entry += 25; // 25% progress per tick
            if *entry >= 100 {
                done.push(entry.key().clone());
            }
        }

        // Mark completed
        for video_id in done {
            self.in_progress.remove(&video_id);
            if let Some(mut video) = videos.get_mut(&video_id) {
                video.status = VideoStatus::Ready;
                video.transcoding_progress = 100;
            }
            completed.push(video_id);
        }

        completed
    }

    fn queue_length(&self) -> usize {
        self.queue.lock().len()
    }
}

// =============================================================================
// CDN (Content Delivery Network)
// =============================================================================

struct CdnNode {
    id: String,
    region: String,
    cache: DashMap<String, Vec<u8>>, // chunk_id -> data
    cache_size: AtomicU64,
    max_cache_size: u64,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl CdnNode {
    fn new(id: &str, region: &str, max_cache_mb: u64) -> Self {
        Self {
            id: id.to_string(),
            region: region.to_string(),
            cache: DashMap::new(),
            cache_size: AtomicU64::new(0),
            max_cache_size: max_cache_mb * 1024 * 1024,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    fn get(&self, chunk_id: &str) -> Option<Vec<u8>> {
        if let Some(data) = self.cache.get(chunk_id) {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Some(data.clone())
        } else {
            self.misses.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    fn put(&self, chunk_id: &str, data: Vec<u8>) {
        let size = data.len() as u64;

        // Simple eviction: if full, don't cache (real: LRU eviction)
        if self.cache_size.load(Ordering::SeqCst) + size > self.max_cache_size {
            return;
        }

        self.cache.insert(chunk_id.to_string(), data);
        self.cache_size.fetch_add(size, Ordering::SeqCst);
    }

    fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::SeqCst);
        let misses = self.misses.load(Ordering::SeqCst);
        if hits + misses == 0 {
            return 0.0;
        }
        hits as f64 / (hits + misses) as f64
    }
}

struct Cdn {
    nodes: DashMap<String, Arc<CdnNode>>,
    origin: DashMap<String, Vec<u8>>, // Origin storage
}

impl Cdn {
    fn new() -> Self {
        Self {
            nodes: DashMap::new(),
            origin: DashMap::new(),
        }
    }

    fn add_node(&self, node: CdnNode) {
        self.nodes.insert(node.id.clone(), Arc::new(node));
    }

    fn upload_to_origin(&self, chunk_id: &str, data: Vec<u8>) {
        self.origin.insert(chunk_id.to_string(), data);
    }

    fn get_chunk(&self, chunk_id: &str, region: &str) -> Option<Vec<u8>> {
        // Find closest CDN node
        let node = self
            .nodes
            .iter()
            .find(|n| n.region == region)
            .or_else(|| self.nodes.iter().next());

        if let Some(node_ref) = node {
            // Try cache first
            if let Some(data) = node_ref.get(chunk_id) {
                return Some(data);
            }

            // Cache miss: fetch from origin
            if let Some(data) = self.origin.get(chunk_id) {
                let data = data.clone();
                node_ref.put(chunk_id, data.clone());
                return Some(data);
            }
        }

        None
    }
}

// =============================================================================
// View Counter (Approximate)
// =============================================================================

struct ViewCounter {
    // Exact counts (for display)
    counts: DashMap<String, AtomicU64>,
    // Buffered increments (batch update)
    buffer: DashMap<String, AtomicU64>,
    buffer_threshold: u64,
}

impl ViewCounter {
    fn new(buffer_threshold: u64) -> Self {
        Self {
            counts: DashMap::new(),
            buffer: DashMap::new(),
            buffer_threshold,
        }
    }

    fn increment(&self, video_id: &str) {
        // Increment buffer
        let buffered = self
            .buffer
            .entry(video_id.to_string())
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
            + 1;

        // Flush to main counter when threshold reached
        if buffered >= self.buffer_threshold {
            self.flush(video_id);
        }
    }

    fn flush(&self, video_id: &str) {
        if let Some((_, buffer)) = self.buffer.remove(video_id) {
            let count = buffer.load(Ordering::SeqCst);
            self.counts
                .entry(video_id.to_string())
                .or_insert(AtomicU64::new(0))
                .fetch_add(count, Ordering::SeqCst);
        }
    }

    fn flush_all(&self) {
        let keys: Vec<String> = self.buffer.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            self.flush(&key);
        }
    }

    fn get(&self, video_id: &str) -> u64 {
        let main = self
            .counts
            .get(video_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0);

        let buffered = self
            .buffer
            .get(video_id)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0);

        main + buffered
    }
}

// =============================================================================
// Video Service
// =============================================================================

struct VideoService {
    videos: Arc<DashMap<String, Video>>,
    variants: DashMap<String, Vec<VideoVariant>>, // video_id -> variants
    transcoder: Arc<TranscodingPipeline>,
    cdn: Arc<Cdn>,
    views: Arc<ViewCounter>,
    video_counter: AtomicU64,
}

impl VideoService {
    fn new() -> Self {
        let cdn = Cdn::new();
        cdn.add_node(CdnNode::new("cdn-us", "us-east", 1000));
        cdn.add_node(CdnNode::new("cdn-eu", "eu-west", 1000));
        cdn.add_node(CdnNode::new("cdn-asia", "asia-east", 1000));

        Self {
            videos: Arc::new(DashMap::new()),
            variants: DashMap::new(),
            transcoder: Arc::new(TranscodingPipeline::new()),
            cdn: Arc::new(cdn),
            views: Arc::new(ViewCounter::new(100)),
            video_counter: AtomicU64::new(0),
        }
    }

    fn upload(&self, title: &str, description: &str, uploader_id: &str, duration_secs: u64) -> String {
        let video_id = format!("vid_{}", self.video_counter.fetch_add(1, Ordering::SeqCst));

        let video = Video {
            id: video_id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            uploader_id: uploader_id.to_string(),
            duration_secs,
            upload_time: Instant::now(),
            status: VideoStatus::Uploading,
            transcoding_progress: 0,
        };

        self.videos.insert(video_id.clone(), video);

        // Submit for transcoding
        self.transcoder.submit(&video_id);

        video_id
    }

    fn process_transcoding(&self) -> Vec<String> {
        let completed = self.transcoder.process_batch(&self.videos);

        // Generate variants for completed videos
        for video_id in &completed {
            self.generate_variants(video_id);
        }

        completed
    }

    fn generate_variants(&self, video_id: &str) {
        let resolutions = vec![
            Resolution::R360p,
            Resolution::R480p,
            Resolution::R720p,
            Resolution::R1080p,
        ];

        let variants: Vec<VideoVariant> = resolutions
            .into_iter()
            .map(|res| {
                // Generate fake chunk IDs
                let chunks: Vec<String> = (0..10)
                    .map(|i| format!("{}_{}_{}", video_id, res as u8, i))
                    .collect();

                // Upload chunks to origin
                for chunk_id in &chunks {
                    let fake_data = vec![0u8; 1000]; // Simulated chunk
                    self.cdn.upload_to_origin(chunk_id, fake_data);
                }

                VideoVariant {
                    video_id: video_id.to_string(),
                    resolution: res,
                    bitrate_kbps: res.bitrate(),
                    chunks,
                }
            })
            .collect();

        self.variants.insert(video_id.to_string(), variants);
    }

    fn watch(&self, video_id: &str, bandwidth_kbps: u32, region: &str) -> Option<Resolution> {
        // Record view
        self.views.increment(video_id);

        // Get variants
        let variants = self.variants.get(video_id)?;

        // Adaptive bitrate: choose best quality for bandwidth
        let best = variants
            .iter()
            .filter(|v| v.bitrate_kbps <= bandwidth_kbps)
            .max_by_key(|v| v.bitrate_kbps)?;

        // Simulate fetching first chunk
        if let Some(chunk_id) = best.chunks.first() {
            self.cdn.get_chunk(chunk_id, region);
        }

        Some(best.resolution)
    }

    fn get_video(&self, video_id: &str) -> Option<Video> {
        self.videos.get(video_id).map(|v| v.clone())
    }

    fn get_views(&self, video_id: &str) -> u64 {
        self.views.get(video_id)
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== YouTube (Video Platform) Demo ===\n");

    let service = VideoService::new();

    // Upload videos
    println!("\n  ═══ Uploading Videos ═══");

    let vid1 = service.upload(
        "How to build distributed systems",
        "Learn system design from scratch",
        "tech_guru",
        600,
    );
    println!("Uploaded: {} (status: {:?})", vid1, VideoStatus::Uploading);

    let vid2 = service.upload(
        "Rust for beginners",
        "Complete Rust tutorial",
        "rust_fan",
        1200,
    );
    println!("Uploaded: {} (status: {:?})", vid2, VideoStatus::Uploading);

    println!("\nTranscoding queue: {}", service.transcoder.queue_length());

    // Process transcoding
    println!("\n--- Transcoding ---");
    for i in 0..5 {
        let completed = service.process_transcoding();
        if !completed.is_empty() {
            println!("Batch {}: Completed {:?}", i + 1, completed);
        }
    }

    // Check video status
    if let Some(video) = service.get_video(&vid1) {
        println!("\n{}: {:?}", video.title, video.status);
    }

    // Show variants
    println!("\n--- Video Variants ---");
    if let Some(variants) = service.variants.get(&vid1) {
        for v in variants.iter() {
            println!(
                "  {:?}: {}kbps ({} chunks)",
                v.resolution,
                v.bitrate_kbps,
                v.chunks.len()
            );
        }
    }

    // Simulate watching
    println!("\n--- Watching Videos ---");

    // User with good connection
    let res = service.watch(&vid1, 8000, "us-east");
    println!("User A (8Mbps, US): watching at {:?}", res);

    // User with slower connection
    let res = service.watch(&vid1, 1500, "eu-west");
    println!("User B (1.5Mbps, EU): watching at {:?}", res);

    // User with poor connection
    let res = service.watch(&vid1, 600, "asia-east");
    println!("User C (600kbps, Asia): watching at {:?}", res);

    // Simulate many views
    println!("\n--- View Counting ---");
    for _ in 0..250 {
        service.views.increment(&vid1);
    }

    // Show view count (buffered + flushed)
    println!("Views for {}: {}", vid1, service.get_views(&vid1));

    // Flush all buffers
    service.views.flush_all();
    println!("After flush: {}", service.get_views(&vid1));

    // CDN stats
    println!("\n--- CDN Stats ---");
    for node in service.cdn.nodes.iter() {
        println!(
            "  {} ({}): {:.1}% hit rate",
            node.id,
            node.region,
            node.hit_rate() * 100.0
        );
    }

    println!("\n=== Key Concepts ===");
    println!("1. Upload Pipeline: Upload -> Queue -> Transcode -> Ready");
    println!("2. Transcoding: Generate multiple resolutions (360p to 4K)");
    println!("3. CDN: Regional caching reduces origin load");
    println!("4. ABR: Adaptive bitrate based on user bandwidth");
    println!("5. View Counting: Buffer + batch flush for high throughput");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcoding_pipeline() {
        let videos = Arc::new(DashMap::new());
        let pipeline = TranscodingPipeline::new();

        videos.insert("v1".to_string(), Video {
            id: "v1".to_string(),
            title: "Test".to_string(),
            description: "".to_string(),
            uploader_id: "u1".to_string(),
            duration_secs: 60,
            upload_time: Instant::now(),
            status: VideoStatus::Uploading,
            transcoding_progress: 0,
        });

        pipeline.submit("v1");
        assert_eq!(pipeline.queue_length(), 1);

        // Process until complete
        for _ in 0..5 {
            pipeline.process_batch(&videos);
        }

        assert_eq!(videos.get("v1").unwrap().status, VideoStatus::Ready);
    }

    #[test]
    fn test_view_counter_buffering() {
        let counter = ViewCounter::new(10);

        // Add 25 views
        for _ in 0..25 {
            counter.increment("v1");
        }

        // Should have flushed at least twice (at 10 and 20)
        let views = counter.get("v1");
        assert_eq!(views, 25);
    }

    #[test]
    fn test_adaptive_bitrate() {
        let service = VideoService::new();

        let vid = service.upload("Test", "Desc", "u1", 60);

        // Process transcoding
        for _ in 0..5 {
            service.process_transcoding();
        }

        // High bandwidth -> high quality
        let res = service.watch(&vid, 10000, "us-east");
        assert_eq!(res, Some(Resolution::R1080p));

        // Low bandwidth -> low quality
        let res = service.watch(&vid, 600, "us-east");
        assert_eq!(res, Some(Resolution::R360p));
    }
}
