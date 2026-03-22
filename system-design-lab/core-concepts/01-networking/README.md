# Networking Essentials

## TCP vs UDP

```
┌─────────────────────────────────────────────────────────────────┐
│                    TCP vs UDP Comparison                         │
│                                                                  │
│  TCP (Transmission Control Protocol)                            │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ • Connection-oriented (3-way handshake)                   │   │
│  │ • Reliable delivery (ACK, retransmission)                 │   │
│  │ • Ordered packets                                         │   │
│  │ • Flow control & congestion control                       │   │
│  │ • Higher latency                                          │   │
│  │                                                           │   │
│  │ Use for: HTTP, databases, file transfer                   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  UDP (User Datagram Protocol)                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ • Connectionless                                          │   │
│  │ • Best-effort delivery (may lose packets)                 │   │
│  │ • No ordering guarantee                                   │   │
│  │ • Minimal overhead                                        │   │
│  │ • Low latency                                             │   │
│  │                                                           │   │
│  │ Use for: DNS, video streaming, gaming, VoIP               │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## TCP 3-Way Handshake

```
Client                                 Server
   │                                      │
   │──────── SYN (seq=x) ────────────────►│
   │                                      │
   │◄─────── SYN-ACK (seq=y, ack=x+1) ────│
   │                                      │
   │──────── ACK (ack=y+1) ──────────────►│
   │                                      │
   │         Connection Established       │
```

## HTTP/1.1 vs HTTP/2 vs HTTP/3

### HTTP/1.1 — The Workhorse (1997)

HTTP/1.1 is **text-based** and sends requests **sequentially** over a TCP connection. Even though it introduced `Connection: keep-alive` (reuse the same TCP connection for multiple requests), it still has a critical flaw: **head-of-line (HOL) blocking**. This means if you send requests A, B, C on one connection, the server MUST respond in order — if A is slow (say a big database query), B and C are stuck waiting even if they're ready.

**Workaround:** Browsers open 6 parallel TCP connections per domain. But each connection costs a full TCP+TLS handshake (~150-300ms), and you're still limited to 6 concurrent requests.

```
Connection 1: ──req A──────────resp A──────req D──resp D──
Connection 2: ──req B──resp B──req E──resp E──────────────
Connection 3: ──req C──resp C──req F──resp F──────────────
                       ↑ 6 TCP connections = 6x handshake cost
```

**Key features:** text headers (verbose, ~800 bytes per request), chunked transfer encoding, keep-alive.

### HTTP/2 — Binary & Multiplexed (2015)

HTTP/2 fixes HOL blocking at the HTTP layer by introducing **streams**. Multiple requests and responses are broken into small **binary frames** and interleaved on a **single TCP connection**. Stream 1's response doesn't block Stream 3's response — they're independent.

**Binary framing** replaces the text format. Every piece of data (headers, body) is wrapped in a frame with a 9-byte header:
```
┌──────────┬────────┬───────┬────────────┬─────────┐
│ Length 3B│Type 1B │Flags 1B│Stream ID 4B│ Payload │
└──────────┴────────┴───────┴────────────┴─────────┘
```

**HPACK header compression** dramatically reduces header size. Common headers like `:method: GET` are sent as a single index byte (1 byte instead of 11). A dynamic table caches headers seen earlier in the connection, so repeated headers cost almost nothing.

**Server push** lets the server proactively send resources before the client asks — e.g., push `style.css` when the client requests `index.html`.

```
Single TCP connection:
  Stream 1 (HTML):  ═══  ═══  ═══
  Stream 3 (CSS):      ═══  ═══  ═══
  Stream 5 (JS):    ═══  ═══  ═══
  (frames interleaved — no HTTP-level HOL blocking!)
```

**The catch:** HTTP/2 still runs over TCP. If a single TCP packet is lost, the OS holds ALL streams until that packet is retransmitted. This is **TCP-level HOL blocking** — one lost packet stalls every stream on the connection. On lossy networks (mobile, WiFi), this can make HTTP/2 *slower* than HTTP/1.1's 6 parallel connections.

### HTTP/3 — QUIC over UDP (2022)

HTTP/3 solves TCP-level HOL blocking by replacing TCP entirely with **QUIC**, a transport protocol built on UDP.

**Why UDP?** TCP is implemented in the OS kernel and can't be changed easily. QUIC runs in userspace over UDP, so it can innovate without waiting for OS updates. QUIC provides everything TCP does (reliable, ordered delivery) but **per-stream** — a lost packet on Stream 1 only blocks Stream 1, not Stream 3.

**Key advantages:**

1. **0-RTT connection establishment.** TCP+TLS needs 2-3 round trips before sending data. QUIC combines transport and TLS 1.3 into one handshake (1 RTT), and on reconnection can send data immediately (0 RTT). That's 100-200ms saved on every new connection.

2. **No HOL blocking across streams.** Each QUIC stream has its own flow control. Packet loss on one stream doesn't affect others. This is the fundamental improvement over HTTP/2.

3. **Connection migration.** TCP connections are identified by (source IP, source port, dest IP, dest port). Switch from WiFi to cellular? Connection dies. QUIC uses a **Connection ID** instead, so connections survive network changes — crucial for mobile.

4. **Built-in encryption.** TLS 1.3 is mandatory and integrated into the handshake. No unencrypted QUIC traffic exists.

```
          TCP+TLS (HTTP/2)              QUIC (HTTP/3)
          ──────────────────           ──────────────────
Handshake: SYN → SYN-ACK → ACK        QUIC Initial →
           ClientHello →               ← QUIC Handshake
           ← ServerHello               (1 RTT total)
           ← Finished
           (2-3 RTT total)

Packet loss on Stream 1:
  TCP:  ALL streams blocked            QUIC: Only Stream 1 blocked
        until retransmit                      Stream 3 keeps flowing

Network switch (WiFi→LTE):
  TCP:  Connection dead, restart        QUIC: Same Connection ID,
                                              seamless continue
```

### Side-by-Side Comparison

| | HTTP/1.1 | HTTP/2 | HTTP/3 |
|---|---|---|---|
| **Year** | 1997 | 2015 | 2022 |
| **Transport** | TCP | TCP | QUIC (UDP) |
| **Format** | Text | Binary frames | Binary frames |
| **Multiplexing** | No (1 req at a time) | Yes (streams) | Yes (streams) |
| **Header compression** | None | HPACK | QPACK |
| **HOL blocking** | HTTP-level | TCP-level | None |
| **Handshake** | 1 RTT (TCP) + 2 RTT (TLS) | Same as HTTP/1.1 | 1 RTT (0-RTT on resume) |
| **Server push** | No | Yes | Yes |
| **Connection migration** | No | No | Yes |
| **Encryption** | Optional (HTTPS) | Effectively required | Always (built-in) |

## HTTP Version Negotiation

How do client and server agree on which HTTP version to use? The mechanism is different for each upgrade path.

### HTTP/1.0 ↔ HTTP/1.1

The client declares the version in the request line (e.g., `GET / HTTP/1.1`). The server responds with the highest version it supports **up to** the client's version. If the client says `HTTP/1.1` but the server only knows `HTTP/1.0`, it responds with `HTTP/1.0` and the client downgrades.

### HTTP/1.1 → HTTP/2

Two paths depending on whether TLS is used:

- **With TLS (HTTPS):** Uses **ALPN** (Application-Layer Protocol Negotiation) — a TLS extension. During the TLS handshake, the client sends a list of supported protocols (`h2, http/1.1`), and the server picks one. This happens *before* any HTTP traffic, so there's zero overhead.
- **Without TLS (rare):** The client sends an HTTP/1.1 request with `Upgrade: h2c` header. If the server supports it, it responds `101 Switching Protocols`. In practice almost nobody does this — HTTP/2 is effectively HTTPS-only.

### HTTP/2 → HTTP/3

HTTP/3 runs on a completely different transport (UDP/QUIC vs TCP), so the upgrade works differently. The server advertises HTTP/3 support via an **`Alt-Svc`** header in an HTTP/2 response:

```
Alt-Svc: h3=":443"; ma=86400
```

This tells the client: "I also speak HTTP/3 on UDP port 443." The client can then *optionally* switch to QUIC for subsequent requests. If QUIC fails (e.g., UDP blocked by a firewall), the client falls back to HTTP/2 over TCP. This is called **happy eyeballs** — try both, use whichever connects first.

### Summary

| Upgrade Path | Mechanism | Where It Happens |
|---|---|---|
| 1.0 ↔ 1.1 | Version in request line | First line of HTTP request |
| 1.1 → 2 (TLS) | **ALPN** in TLS handshake | During TLS negotiation |
| 1.1 → 2 (plain) | `Upgrade: h2c` header | HTTP request/response (rare) |
| 2 → 3 | `Alt-Svc` header | HTTP response from server |

The key insight: **the client always proposes, the server decides**. And there's always a fallback — if the server doesn't support a newer version, the connection works at the older version. This is why HTTP versions are backward-compatible and the internet didn't break when HTTP/2 and HTTP/3 rolled out.

## Head-of-Line Blocking — What It Actually Means

A common misconception: "HTTP/1.1 HOL blocking means 1000 requests wait for the slowest one." **That's not how it works.** HOL blocking is **per-connection**, not per-client.

### HTTP/1.1: Sequential Per Connection

On a **single TCP connection**, requests are strictly sequential:

```
Connection 1:  req1 → resp1 → req2 → resp2 → req3 → resp3
```

If `req1` takes 500ms (e.g., a slow database query), `req2` and `req3` are stuck waiting **on that connection**. This is head-of-line blocking.

But the client can open **multiple TCP connections** in parallel. Browsers open 6 connections per domain by default:

```
Connection 1: req1(slow) ─────────── resp1 → req7 → resp7 → ...
Connection 2: req2 → resp2 → req8 → resp8 → ...
Connection 3: req3 → resp3 → req9 → resp9 → ...
Connection 4: req4 → resp4 → req10 → resp10 → ...
Connection 5: req5 → resp5 → req11 → resp11 → ...
Connection 6: req6 → resp6 → req12 → resp12 → ...
```

If `req1` on Connection 1 is slow, **only Connection 1 is blocked**. Connections 2-6 keep going. With 1000 requests and 6 connections, you process ~6 at a time.

**The problem:** each extra connection costs a full TCP + TLS handshake (~150-300ms), and you're still limited to 6 concurrent requests per domain. That's 6 pipelines, each sequential.

### HTTP/2: All Requests Fly in Parallel on ONE Connection

HTTP/2 multiplexes **all requests** on a single TCP connection as independent streams:

```
ONE connection:
  Stream 1 (/slow):   ════════════════════════ (500ms)
  Stream 3 (/health): ══ (1ms, done immediately)
  Stream 5 (/api):    ═══ (2ms, done immediately)
  Stream 7 (/users):  ═══ (2ms, done immediately)
  ... all 1000 requests can be in-flight simultaneously
```

`/slow` taking 500ms doesn't block any other stream. No need for 6 connections — one connection handles all of them.

**Total time comparison for 1000 requests where one is slow (500ms):**

| | HTTP/1.1 (6 connections) | HTTP/2 (1 connection) |
|---|---|---|
| Connections needed | 6 TCP connections | 1 TCP connection |
| Handshake cost | 6 × (TCP + TLS) = ~1.5s | 1 × (TCP + TLS) = ~250ms |
| Slow request impact | Blocks 1 of 6 pipelines | Blocks only its own stream |
| Total time | ~500ms + (994 reqs / 6) × avg_time | ~max(500ms, all other reqs) |

## WebSocket

```
┌─────────────────────────────────────────────────────────────────┐
│                      WebSocket Protocol                          │
│                                                                  │
│  Client                               Server                     │
│     │                                    │                       │
│     │──── HTTP Upgrade Request ─────────►│                       │
│     │     Connection: Upgrade            │                       │
│     │     Upgrade: websocket             │                       │
│     │                                    │                       │
│     │◄─── HTTP 101 Switching ────────────│                       │
│     │                                    │                       │
│     │═══════ Full-duplex channel ════════│                       │
│     │         (persistent)               │                       │
│     │                                    │                       │
│     │──────► Message ◄──────────────────│                       │
│     │◄────── Message ─────────►         │                       │
│     │                                    │                       │
│                                                                  │
│  Use cases:                                                      │
│  • Real-time chat                                               │
│  • Live updates (stock prices, sports)                          │
│  • Collaborative editing                                        │
│  • Gaming                                                       │
└─────────────────────────────────────────────────────────────────┘
```

## Long Polling vs SSE vs WebSocket

| Method | Direction | Use Case | Overhead |
|--------|-----------|----------|----------|
| **Long Polling** | Server → Client | Legacy real-time | High (reconnect) |
| **SSE** | Server → Client | Notifications | Low |
| **WebSocket** | Bidirectional | Chat, gaming | Very low |

## DNS Resolution

```
┌─────────────────────────────────────────────────────────────────┐
│                     DNS Resolution Flow                          │
│                                                                  │
│  Browser                                                         │
│     │                                                           │
│     │ 1. Check browser cache                                    │
│     │                                                           │
│     │ 2. Check OS cache                                         │
│     │                                                           │
│     ▼                                                           │
│  ┌─────────────────┐                                            │
│  │ Local Resolver  │ 3. Check resolver cache                    │
│  └────────┬────────┘                                            │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────┐                                            │
│  │  Root Server    │ 4. "Ask .com server"                       │
│  └────────┬────────┘                                            │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────┐                                            │
│  │   TLD Server    │ 5. "Ask example.com NS"                    │
│  │    (.com)       │                                            │
│  └────────┬────────┘                                            │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────┐                                            │
│  │ Authoritative   │ 6. "IP is 93.184.216.34"                   │
│  │    Server       │                                            │
│  └─────────────────┘                                            │
│                                                                  │
│  Total: ~200ms (uncached), <1ms (cached)                        │
└─────────────────────────────────────────────────────────────────┘
```

## CDN (Content Delivery Network)

```
┌─────────────────────────────────────────────────────────────────┐
│                     CDN Architecture                             │
│                                                                  │
│                    ┌──────────────────┐                         │
│                    │   Origin Server  │                         │
│                    │   (your server)  │                         │
│                    └────────┬─────────┘                         │
│                             │                                    │
│            ┌────────────────┼────────────────┐                  │
│            │                │                │                  │
│            ▼                ▼                ▼                  │
│     ┌───────────┐    ┌───────────┐    ┌───────────┐            │
│     │ Edge Node │    │ Edge Node │    │ Edge Node │            │
│     │  (NYC)    │    │ (London)  │    │ (Tokyo)   │            │
│     └─────┬─────┘    └─────┬─────┘    └─────┬─────┘            │
│           │                │                │                   │
│           ▼                ▼                ▼                   │
│        Users             Users           Users                  │
│                                                                  │
│  Benefits:                                                      │
│  • Lower latency (content served from nearby edge)              │
│  • Reduced origin load                                          │
│  • DDoS protection                                              │
│  • High availability                                            │
└─────────────────────────────────────────────────────────────────┘
```

## Load Balancing

See the dedicated load balancer module for details.

## Implementation

Our demo includes:
1. TCP echo server
2. Basic HTTP server
3. Simple WebSocket-like protocol

Run the demo:
```bash
cargo run --bin networking
```
