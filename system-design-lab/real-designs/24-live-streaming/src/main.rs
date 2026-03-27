#![allow(dead_code, unused_variables, unused_imports)]
//! # Live Streaming Platform — Mini Implementation
//!
//! Simulates the core pipeline of a live streaming system:
//! 1. Ingest: streamer pushes video frames
//! 2. Transcoding: convert to multiple quality levels
//! 3. Segmenter: split into HLS segments
//! 4. CDN: distribute segments, cache at edge
//! 5. Viewers: request segments, adaptive bitrate
//! 6. Live chat: fan-out messages to viewers
//! 7. Viewer count: real-time tracking
//!
//! Run: cargo run -p live-streaming

use dashmap::DashMap;
use rand::Rng;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone)]
struct Stream {
    id: String,
    streamer: String,
    title: String,
    status: StreamStatus,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum StreamStatus {
    Live,
    Ended,
}

#[derive(Debug, Clone)]
struct VideoSegment {
    stream_id: String,
    sequence: u64,
    quality: Quality,
    duration_ms: u64,
    size_bytes: usize,
    data: Vec<u8>, // simulated segment data
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Quality {
    Q1080p,
    Q720p,
    Q480p,
    Q360p,
}

impl Quality {
    fn bitrate_kbps(&self) -> u32 {
        match self {
            Quality::Q1080p => 6000,
            Quality::Q720p => 3000,
            Quality::Q480p => 1500,
            Quality::Q360p => 500,
        }
    }

    fn label(&self) -> &str {
        match self {
            Quality::Q1080p => "1080p",
            Quality::Q720p => "720p",
            Quality::Q480p => "480p",
            Quality::Q360p => "360p",
        }
    }

    fn all() -> &'static [Quality] {
        &[
            Quality::Q1080p,
            Quality::Q720p,
            Quality::Q480p,
            Quality::Q360p,
        ]
    }
}

#[derive(Debug, Clone)]
struct ChatMessage {
    stream_id: String,
    user: String,
    message: String,
    timestamp: Instant,
}

// =============================================================================
// 1. Ingest Server — receives raw stream from broadcaster
// =============================================================================

struct IngestServer {
    streams: DashMap<String, Stream>,
    raw_frames: DashMap<String, VecDeque<Vec<u8>>>,
}

impl IngestServer {
    fn new() -> Self {
        Self {
            streams: DashMap::new(),
            raw_frames: DashMap::new(),
        }
    }

    fn start_stream(&self, streamer: &str, title: &str) -> String {
        let id = Uuid::new_v4().to_string()[..8].to_string();
        self.streams.insert(
            id.clone(),
            Stream {
                id: id.clone(),
                streamer: streamer.into(),
                title: title.into(),
                status: StreamStatus::Live,
                started_at: Instant::now(),
            },
        );
        self.raw_frames.insert(id.clone(), VecDeque::new());
        id
    }

    // Simulate receiving a raw video frame from OBS/phone
    fn push_frame(&self, stream_id: &str, frame_data: Vec<u8>) {
        if let Some(mut frames) = self.raw_frames.get_mut(stream_id) {
            frames.push_back(frame_data);
        }
    }

    fn pop_frame(&self, stream_id: &str) -> Option<Vec<u8>> {
        self.raw_frames.get_mut(stream_id)?.pop_front()
    }
}

// =============================================================================
// 2. Transcoding Pipeline — convert to multiple quality levels
// =============================================================================

struct Transcoder;

impl Transcoder {
    // Simulate transcoding: raw frame → multiple quality segments
    fn transcode(raw_frame: &[u8], sequence: u64, stream_id: &str) -> Vec<VideoSegment> {
        Quality::all()
            .iter()
            .map(|&quality| {
                let ratio = quality.bitrate_kbps() as f64 / Quality::Q1080p.bitrate_kbps() as f64;
                let size = (raw_frame.len() as f64 * ratio) as usize;
                VideoSegment {
                    stream_id: stream_id.to_string(),
                    sequence,
                    quality,
                    duration_ms: 2000, // 2-second segments
                    size_bytes: size,
                    data: vec![0u8; size], // simulated data
                }
            })
            .collect()
    }
}

// =============================================================================
// 3. Origin Server — stores segments, serves to CDN
// =============================================================================

struct OriginServer {
    // stream_id:quality:sequence → segment
    segments: DashMap<String, VideoSegment>,
    serve_count: AtomicU64,
}

impl OriginServer {
    fn new() -> Self {
        Self {
            segments: DashMap::new(),
            serve_count: AtomicU64::new(0),
        }
    }

    fn store_segment(&self, segment: VideoSegment) {
        let key = format!(
            "{}:{}:{}",
            segment.stream_id,
            segment.quality.label(),
            segment.sequence
        );
        self.segments.insert(key, segment);
    }

    fn get_segment(
        &self,
        stream_id: &str,
        quality: Quality,
        sequence: u64,
    ) -> Option<VideoSegment> {
        let key = format!("{}:{}:{}", stream_id, quality.label(), sequence);
        self.serve_count.fetch_add(1, Ordering::Relaxed);
        self.segments.get(&key).map(|s| s.clone())
    }
}

// =============================================================================
// 4. CDN Edge — caches segments close to viewers
// =============================================================================

struct CdnEdge {
    name: String,
    cache: DashMap<String, VideoSegment>,
    origin: Arc<OriginServer>,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl CdnEdge {
    fn new(name: &str, origin: Arc<OriginServer>) -> Self {
        Self {
            name: name.into(),
            cache: DashMap::new(),
            origin,
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }

    fn get_segment(
        &self,
        stream_id: &str,
        quality: Quality,
        sequence: u64,
    ) -> Option<VideoSegment> {
        let key = format!("{}:{}:{}", stream_id, quality.label(), sequence);

        // Check edge cache first
        if let Some(seg) = self.cache.get(&key) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            return Some(seg.clone());
        }

        // Cache miss → fetch from origin
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        if let Some(seg) = self.origin.get_segment(stream_id, quality, sequence) {
            self.cache.insert(key, seg.clone());
            Some(seg)
        } else {
            None
        }
    }
}

// =============================================================================
// 5. Live Chat — fan-out messages
// =============================================================================

struct ChatService {
    messages: DashMap<String, Vec<ChatMessage>>,
    total_messages: AtomicU64,
}

impl ChatService {
    fn new() -> Self {
        Self {
            messages: DashMap::new(),
            total_messages: AtomicU64::new(0),
        }
    }

    fn send(&self, stream_id: &str, user: &str, message: &str) {
        self.messages
            .entry(stream_id.to_string())
            .or_default()
            .push(ChatMessage {
                stream_id: stream_id.to_string(),
                user: user.into(),
                message: message.into(),
                timestamp: Instant::now(),
            });
        self.total_messages.fetch_add(1, Ordering::Relaxed);
    }

    fn get_recent(&self, stream_id: &str, count: usize) -> Vec<ChatMessage> {
        self.messages
            .get(stream_id)
            .map(|msgs| msgs.iter().rev().take(count).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    }
}

// =============================================================================
// 6. Viewer Counter — real-time tracking
// =============================================================================

struct ViewerCounter {
    counts: DashMap<String, AtomicUsize>,
    peak: DashMap<String, AtomicUsize>,
}

impl ViewerCounter {
    fn new() -> Self {
        Self {
            counts: DashMap::new(),
            peak: DashMap::new(),
        }
    }

    fn join(&self, stream_id: &str) {
        let count = self
            .counts
            .entry(stream_id.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        let new_count = count.fetch_add(1, Ordering::Relaxed) + 1;

        let peak = self
            .peak
            .entry(stream_id.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        peak.fetch_max(new_count, Ordering::Relaxed);
    }

    fn leave(&self, stream_id: &str) {
        if let Some(count) = self.counts.get(stream_id) {
            count.fetch_sub(1, Ordering::Relaxed);
        }
    }

    fn get_count(&self, stream_id: &str) -> usize {
        self.counts
            .get(stream_id)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    fn get_peak(&self, stream_id: &str) -> usize {
        self.peak
            .get(stream_id)
            .map(|p| p.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

// =============================================================================
// Demo
// =============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════╗");
    println!("║      Live Streaming Platform Simulation          ║");
    println!("╚══════════════════════════════════════════════════╝\n");

    // Setup infrastructure
    let ingest = Arc::new(IngestServer::new());
    let origin = Arc::new(OriginServer::new());
    let chat = Arc::new(ChatService::new());
    let viewers = Arc::new(ViewerCounter::new());

    // CDN edges in multiple regions
    let cdn_us = Arc::new(CdnEdge::new("CDN-US-East", Arc::clone(&origin)));
    let cdn_eu = Arc::new(CdnEdge::new("CDN-Europe", Arc::clone(&origin)));
    let cdn_asia = Arc::new(CdnEdge::new("CDN-Asia", Arc::clone(&origin)));

    // ── Step 1: Streamer starts broadcasting ──
    println!("━━━ 1. Ingest — Streamer Starts Broadcasting ━━━\n");
    let stream_id = ingest.start_stream("ninja", "Epic Fortnite Stream");
    println!("    Stream started: id={}", stream_id);
    println!("    Streamer pushes RTMP to nearest ingest server.\n");

    // Simulate pushing 5 raw frames
    for i in 0..5 {
        let frame_size = 200_000 + rand::thread_rng().gen_range(0..50_000); // ~200KB per frame
        ingest.push_frame(&stream_id, vec![0u8; frame_size]);
    }
    println!("    Pushed 5 raw video frames to ingest server.\n");

    // ── Step 2: Transcode to multiple qualities ──
    println!("━━━ 2. Transcoding — Multiple Quality Levels ━━━\n");
    let mut sequence = 0u64;
    while let Some(raw_frame) = ingest.pop_frame(&stream_id) {
        let segments = Transcoder::transcode(&raw_frame, sequence, &stream_id);
        for seg in &segments {
            origin.store_segment(seg.clone());
        }
        if sequence == 0 {
            println!("    Raw frame: {} bytes", raw_frame.len());
            println!("    Transcoded to:");
            for seg in &segments {
                println!(
                    "      {} → {} bytes ({}kbps)",
                    seg.quality.label(),
                    seg.size_bytes,
                    seg.quality.bitrate_kbps()
                );
            }
            println!();
        }
        sequence += 1;
    }
    println!(
        "    Transcoded {} segments × {} qualities = {} total segments stored.\n",
        sequence,
        Quality::all().len(),
        sequence * Quality::all().len() as u64
    );

    // ── Step 3: CDN distribution ──
    println!("━━━ 3. CDN — Edge Caching & Distribution ━━━\n");

    // Simulate 100 viewers across 3 regions requesting segments
    let rng = rand::thread_rng();
    let edges = [&cdn_us, &cdn_eu, &cdn_asia];
    let viewer_distribution = [50, 30, 20]; // US: 50, EU: 30, Asia: 20

    for (edge, count) in edges.iter().zip(viewer_distribution.iter()) {
        for _ in 0..*count {
            // Each viewer watches all 5 segments at 720p
            for seq in 0..sequence {
                edge.get_segment(&stream_id, Quality::Q720p, seq);
            }
            viewers.join(&stream_id);
        }
    }

    println!("    100 viewers across 3 CDN regions:");
    for edge in &edges {
        let hits = edge.cache_hits.load(Ordering::Relaxed);
        let misses = edge.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        println!(
            "    {}: {} requests, {} hits, {} misses ({:.1}% hit rate)",
            edge.name, total, hits, misses, hit_rate
        );
    }

    let origin_serves = origin.serve_count.load(Ordering::Relaxed);
    println!(
        "\n    Origin served {} requests (CDN absorbed the rest)",
        origin_serves
    );
    println!(
        "    Without CDN: {} requests would hit origin\n",
        100 * sequence
    ); // every viewer × every segment

    // ── Step 4: Adaptive bitrate ──
    println!("━━━ 4. Adaptive Bitrate — Quality Switching ━━━\n");

    let qualities_by_bandwidth = [
        (10000, Quality::Q1080p),
        (4000, Quality::Q720p),
        (2000, Quality::Q480p),
        (800, Quality::Q360p),
    ];

    println!("    Viewer's network fluctuates → player switches quality:\n");
    let bandwidth_samples = [8000, 5000, 1500, 3500, 7000];
    for (i, &bw) in bandwidth_samples.iter().enumerate() {
        let quality = qualities_by_bandwidth
            .iter()
            .find(|(min_bw, _)| bw >= *min_bw)
            .map(|(_, q)| q)
            .unwrap_or(&Quality::Q360p);

        let seg = cdn_us.get_segment(&stream_id, *quality, i as u64 % sequence);
        if let Some(seg) = seg {
            println!(
                "    Segment {}: bandwidth={}kbps → {} ({} bytes)",
                i,
                bw,
                quality.label(),
                seg.size_bytes
            );
        }
    }
    println!("\n    Player automatically picks best quality for current bandwidth.\n");

    // ── Step 5: Live chat ──
    println!("━━━ 5. Live Chat — Message Fan-out ━━━\n");

    let messages = [
        ("viewer42", "LET'S GOOO"),
        ("mod_sarah", "Welcome everyone!"),
        ("viewer99", "First time watching"),
        ("subscriber_bob", "gifted 5 subs!"),
        ("viewer123", "PogChamp"),
        ("viewer456", "GG"),
    ];

    for (user, msg) in &messages {
        chat.send(&stream_id, user, msg);
    }

    println!(
        "    {} messages sent",
        chat.total_messages.load(Ordering::Relaxed)
    );
    println!("    Recent messages (most recent first):");
    for msg in chat.get_recent(&stream_id, 4) {
        println!("      {}: {}", msg.user, msg.message);
    }

    // ── Step 6: Viewer count ──
    println!("\n━━━ 6. Viewer Count ━━━\n");
    println!("    Current viewers: {}", viewers.get_count(&stream_id));
    println!("    Peak viewers:    {}", viewers.get_peak(&stream_id));

    // Some viewers leave
    for _ in 0..20 {
        viewers.leave(&stream_id);
    }
    println!(
        "    After 20 viewers leave: {}",
        viewers.get_count(&stream_id)
    );

    // ── Summary ──
    println!("\n━━━ Architecture Summary ━━━\n");
    println!("    ┌─────────────────┬────────────────────────────────────┐");
    println!("    │ Component       │ What it does                       │");
    println!("    ├─────────────────┼────────────────────────────────────┤");
    println!("    │ Ingest Server   │ Receives RTMP from streamer        │");
    println!("    │ Transcoder      │ Raw → 1080p/720p/480p/360p         │");
    println!("    │ Origin Server   │ Stores HLS segments                │");
    println!("    │ CDN Edge        │ Caches segments near viewers       │");
    println!("    │ Chat Service    │ WebSocket fan-out + sampling       │");
    println!("    │ Viewer Counter  │ Redis INCR/DECR per stream         │");
    println!("    └─────────────────┴────────────────────────────────────┘\n");

    println!("╔══════════════════════════════════════════════════╗");
    println!("║              Demo Complete!                      ║");
    println!("╚══════════════════════════════════════════════════╝");
}
