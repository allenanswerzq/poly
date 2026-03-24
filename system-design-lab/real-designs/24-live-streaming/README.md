# Design Live Streaming (Twitch / YouTube Live / Instagram Live)

## Problem Statement

Design a live streaming platform that supports:
- Streamers broadcasting live video to thousands/millions of viewers
- Sub-second latency for real-time interaction
- Live chat alongside the stream
- Stream recording for replay (VOD)
- Adaptive bitrate for different network conditions

## Requirements

### Functional
- Streamer starts a live stream (push video from OBS/phone)
- Viewers watch in real-time (<5 second latency)
- Live chat with the stream
- Viewer count (real-time)
- Stream quality adapts to viewer's network
- VOD (video on demand) after stream ends

### Non-Functional
- Scale: 100K concurrent viewers per stream, 10K concurrent streams
- Latency: <5s glass-to-glass (camera → viewer screen)
- Availability: 99.9% (stream can't drop during live event)
- Global: viewers worldwide via CDN

## High-Level Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────────┐
│  Streamer    │────►│ Ingest Server │────►│ Transcoding Pipeline │
│  (OBS/Phone) │ RTMP│  (edge, near  │     │ (multiple qualities) │
└─────────────┘     │   streamer)   │     └──────────┬──────────┘
                    └──────────────┘                  │
                                                      │ HLS/DASH segments
                                              ┌───────▼────────┐
                                              │   Origin Server  │
                                              │  (segment store) │
                                              └───────┬────────┘
                                                      │
                                    ┌─────────────────┼─────────────────┐
                                    │                 │                 │
                              ┌─────▼─────┐   ┌──────▼────┐   ┌──────▼────┐
                              │  CDN Edge  │   │ CDN Edge   │   │ CDN Edge   │
                              │  (US-East) │   │ (Europe)   │   │ (Asia)     │
                              └─────┬─────┘   └──────┬────┘   └──────┬────┘
                                    │                 │                │
                              ┌─────▼─────┐   ┌──────▼────┐   ┌──────▼────┐
                              │  Viewers   │   │  Viewers   │   │  Viewers   │
                              │  (100K+)   │   │  (50K+)    │   │  (30K+)    │
                              └───────────┘   └───────────┘   └───────────┘
```

## Key Components

### 1. Ingest Server
- Receives RTMP stream from broadcaster
- Located near the streamer (edge POP)
- Validates stream key, authenticates streamer
- Forwards raw video to transcoding pipeline

### 2. Transcoding Pipeline
```
Raw stream (1080p 8Mbps)
    │
    ├──► 1080p @ 6Mbps   (high quality)
    ├──► 720p  @ 3Mbps   (medium)
    ├──► 480p  @ 1.5Mbps (low)
    └──► 360p  @ 0.5Mbps (mobile/slow network)

Each quality → split into 2-6 second HLS segments
Segment: stream_abc_720p_00042.ts
```

### 3. HLS/DASH Delivery
```
Viewer requests:
  1. GET /stream/abc/master.m3u8     → playlist of available qualities
  2. GET /stream/abc/720p/index.m3u8 → playlist of segments for 720p
  3. GET /stream/abc/720p/00042.ts   → actual video segment (2-6 sec)

Player fetches new segments every few seconds.
Adaptive: if network slows, switch to 480p automatically.
```

### 4. CDN Distribution
```
Viewer in Tokyo requests segment:
  1. CDN edge (Tokyo) → has it cached? YES → serve immediately
  2. CDN edge (Tokyo) → cache miss → request from origin
  3. Origin → has it? → serve to CDN edge → CDN caches it
  4. Next viewer in Tokyo → CDN cache hit

1 million viewers watching same stream ≠ 1 million origin requests.
CDN absorbs 99%+ of the load.
```

### 5. Live Chat (separate system)
```
Viewer sends message → Chat Service → fan-out to all viewers

At 100K viewers, you can't send every message to every viewer.
Solutions:
  - Sample: show ~20 messages/sec from the firehose
  - Buckets: shard viewers into groups, each sees a subset
  - Top messages: only show messages with reactions/from mods
```

## Database Schema

```sql
-- Streams
CREATE TABLE streams (
    id UUID PRIMARY KEY,
    streamer_id UUID NOT NULL,
    title TEXT NOT NULL,
    status VARCHAR NOT NULL,  -- 'live', 'ended', 'processing'
    stream_key VARCHAR UNIQUE NOT NULL,
    started_at TIMESTAMP,
    ended_at TIMESTAMP,
    viewer_count_peak INTEGER DEFAULT 0
);

-- Chat messages (Cassandra or similar — high write throughput)
-- Partition by stream_id, cluster by timestamp
CREATE TABLE chat_messages (
    stream_id UUID,
    sent_at TIMESTAMP,
    user_id UUID,
    message TEXT,
    PRIMARY KEY (stream_id, sent_at)
) WITH CLUSTERING ORDER BY (sent_at DESC);

-- Viewer count (Redis)
-- INCR stream:{id}:viewers   (on join)
-- DECR stream:{id}:viewers   (on leave)
-- GET  stream:{id}:viewers   (display)
```

## Latency Breakdown

```
Camera capture      →  ~30ms (encoding)
Upload to ingest    → ~100ms (network, depends on streamer location)
Transcoding         → ~500ms (segment creation)
Origin → CDN        → ~100ms (first viewer in region)
CDN → Viewer        →  ~50ms (cached)
Player buffering    → ~2-4s  (player holds 2-3 segments)
────────────────────────────────
Total glass-to-glass: ~3-5 seconds

Lower latency options:
  WebRTC:  <1s latency, but hard to scale beyond ~1000 viewers
  LL-HLS:  ~2s latency, works with CDN at scale
  RTMP:    ~1-2s, but not natively supported in browsers
```

## Scaling Challenges

| Challenge | Solution |
|-----------|----------|
| 1M viewers, 1 stream | CDN caching (origin hit once per segment per edge) |
| 10K concurrent streams | Horizontal ingest servers, auto-scaling transcoders |
| Global viewers | Multi-region CDN (CloudFront, Akamai, Cloudflare) |
| Chat at scale | Shard by stream, sample messages, use WebSocket |
| Viewer count | Redis INCR/DECR, approximate (HyperLogLog for unique) |
| Stream recording | Write segments to S3 as they're created, assemble VOD after |
| Streamer disconnect | Reconnect window (30s), resume from last segment |

## Interview Talking Points

> "The streamer pushes RTMP to the nearest ingest server. The transcoding pipeline creates multiple quality levels as HLS segments. Segments are pushed to an origin server and distributed via CDN. Viewers request segments from the CDN edge — the CDN absorbs all the fan-out so the origin only serves each segment once per edge location. For a stream with 1 million viewers across 50 CDN edges, the origin only serves ~50 requests per segment, not 1 million."

> "For live chat at scale, I'd use a WebSocket gateway that fans out messages. At 100K viewers, showing every message would overwhelm the UI, so I'd sample to ~20 messages/second and prioritize moderator and subscriber messages."

> "Latency is 3-5 seconds with HLS. If we need sub-second for interactive streams (gaming, auctions), I'd use WebRTC for a small audience or LL-HLS with partial segments for larger audiences."
