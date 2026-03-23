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

## What Does Each Side Need to Support These Protocols?

A common misunderstanding: "the client just sets a flag and it works." **Both sides must implement the full protocol.** Here's what the server and client each need for every protocol we've covered:

### HTTP/1.1

| | Server | Client |
|---|---|---|
| **Parse** | Read text request line + headers (`GET / HTTP/1.1\r\n...`) | Read text status line + headers (`HTTP/1.1 200 OK\r\n...`) |
| **Body handling** | `Content-Length` or `Transfer-Encoding: chunked` | Same — read exact bytes or decode chunked |
| **Keep-alive** | Don't close the socket after response; read next request | Reuse same TCP socket for next request (connection pool) |
| **Library** | Any HTTP server (axum/hyper, nginx, Express) | Any HTTP client (reqwest, curl, fetch) |

Simple text protocol — most languages have it built in.

### HTTP/2

| | Server | Client |
|---|---|---|
| **Binary framing** | Parse/encode 9-byte frame headers + payloads | Same — encode requests as HEADERS/DATA frames |
| **Stream multiplexing** | Track N concurrent streams per connection | Match incoming frames to their stream IDs |
| **HPACK compression** | Maintain static + dynamic header tables | Same — compress outgoing, decompress incoming |
| **Flow control** | Send/receive WINDOW_UPDATE per stream | Same — respect receive windows, send updates |
| **Connection preface** | Detect `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` | Send this 24-byte magic on connect |
| **Negotiation** | ALPN support in TLS library | Propose `h2` in ALPN, or use h2c (prior knowledge) |
| **Library** | hyper, h2 (Rust), nghttp2 (C), net/http (Go) | reqwest/h2 (Rust), libcurl, browsers |

In our demo, **axum/hyper** handles all server-side HTTP/2 automatically. **reqwest/h2** handles the client side. `.http2_prior_knowledge()` tells reqwest to send the connection preface directly.

### HTTP/3 / QUIC

| | Server | Client |
|---|---|---|
| **Transport** | Listen on UDP (not TCP!) | Connect via UDP |
| **QUIC implementation** | Full QUIC state machine: connection IDs, packet numbers, crypto, per-stream loss recovery | Same — implement QUIC as a client |
| **TLS 1.3** | Built into QUIC handshake (mandatory, not optional) | Same |
| **Stream multiplexing** | Independent streams over UDP (no TCP HOL blocking) | Same |
| **Connection migration** | Accept packets from new IP with same connection ID | Re-send from new IP after WiFi→LTE switch |
| **Advertisement** | Send `Alt-Svc: h3=":443"` in HTTP/2 responses | Parse `Alt-Svc`, try QUIC, fallback to TCP |
| **Library** | quinn (Rust), quiche (C), msquic (Microsoft) | Same libraries, plus browsers natively |

HTTP/3 is the hardest to deploy — you need a full QUIC stack on both sides, and your network must allow UDP.

### WebSocket

| | Server | Client |
|---|---|---|
| **Handshake** | Parse `Upgrade: websocket` + `Sec-WebSocket-Key`, respond with `101` + SHA-1 accept hash | Send HTTP Upgrade request with random `Sec-WebSocket-Key` |
| **Framing** | Parse/encode WebSocket frames: FIN, opcode, mask, payload length | Same — client frames MUST be masked (server frames are NOT) |
| **Opcodes** | Handle Text (0x1), Binary (0x2), Close (0x8), Ping (0x9), Pong (0xA) | Same |
| **Ping/Pong** | Respond to Ping with Pong (keep-alive) | Send Ping, expect Pong |
| **Close** | Send/receive Close frames for graceful shutdown | Same |
| **Library** | tokio-tungstenite, ws (Node), gorilla/websocket (Go) | tungstenite, browser WebSocket API, ws |

Our demo implements the full handshake + framing from scratch to show every byte on the wire.

### SSE (Server-Sent Events)

| | Server | Client |
|---|---|---|
| **Response** | Set `Content-Type: text/event-stream`, keep connection open | Send request with `Accept: text/event-stream` |
| **Event format** | Write `id: N\nevent: type\ndata: payload\n\n` | Parse the `id:`, `event:`, `data:` fields |
| **Reconnection** | Handle `Last-Event-ID` header on reconnect | Auto-reconnect with `Last-Event-ID` (browser does this) |
| **Library** | Any HTTP server (just keep writing to the response) | Browser `EventSource` API, or manual parsing |

SSE is the simplest — it's just a long-lived HTTP response. The server writes text lines, the client reads them.

### The Pattern

Across all protocols:
- **Client proposes** the protocol (request line, ALPN, Upgrade header, Alt-Svc)
- **Server decides** whether it supports it
- **Both sides must implement** the wire format (framing, compression, flow control)
- **Your application code doesn't change** — the library handles the protocol layer

```
Your code:    GET /api/users → JSON response     (same for ALL protocols)
                    ↕                                        ↕
Library:      HTTP/1.1 text    HTTP/2 binary    HTTP/3 QUIC    WebSocket frames
                    ↕              ↕                ↕              ↕
Wire:           TCP text       TCP binary       UDP binary     TCP binary
```

## HTTP/2 Large Payloads — Framing and Streaming

What happens when you upload a 1GB file over HTTP/2? It does **NOT** load everything into memory. HTTP/2 uses **chunked DATA frames** with flow control:

### How It Works

A large payload is split into multiple DATA frames (default max 16KB each):

```
Upload 1GB file on Stream 5:

  HEADERS frame (stream 5): POST /upload Content-Length: 1073741824
  DATA frame (stream 5):    [16KB chunk 1]
  DATA frame (stream 5):    [16KB chunk 2]
  DATA frame (stream 3):    [response to another request — interleaved!]
  DATA frame (stream 5):    [16KB chunk 3]
  DATA frame (stream 5):    [16KB chunk 4]
  ...65,536 DATA frames later...
  DATA frame (stream 5, END_STREAM): [last chunk]
```

Key points:
1. **Streaming**: Data flows through frame-by-frame. Neither client nor server needs 1GB of RAM for the transfer.
2. **Interleaving**: Other streams' frames can be mixed in between your upload's DATA frames. A 1GB upload doesn't block other requests.
3. **Flow control**: Each stream has a **receive window** (default 64KB). The receiver sends WINDOW_UPDATE frames to say "I've consumed some data, send me more." This prevents the sender from overwhelming a slow receiver.
4. **Backpressure**: If the receiver stops sending WINDOW_UPDATE, the sender pauses that stream — without affecting other streams.

### Flow Control Windows

```
Stream 5 flow:

  Sender                              Receiver
    │                                    │
    │── DATA [16KB] ────────────────────►│  window: 64KB → 48KB
    │── DATA [16KB] ────────────────────►│  window: 48KB → 32KB
    │── DATA [16KB] ────────────────────►│  window: 32KB → 16KB
    │── DATA [16KB] ────────────────────►│  window: 16KB → 0KB
    │                                    │
    │   (sender PAUSES — window full)    │
    │                                    │
    │◄── WINDOW_UPDATE [32KB] ──────────│  receiver processed some data
    │                                    │  window: 0KB → 32KB
    │── DATA [16KB] ────────────────────►│  window: 32KB → 16KB
    │── DATA [16KB] ────────────────────►│  window: 16KB → 0KB
    │   ...continues...                  │
```

This is why HTTP/2 is safe for streaming large files, video, or gRPC bidirectional streams — it's designed from the ground up for this.

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

### Half-Duplex vs Full-Duplex — Why WebSocket Exists

Normal HTTP is **request-response** (half-duplex): the client sends a request, then waits, then receives a response. The server can **never** initiate a message — it can only respond to client requests.

```
HTTP/1.1 (half-duplex, sequential):
  Client: [send req ──►] [wait...] [◄── recv resp] [send req ──►] [wait...]
  Server:                 [process]  [send resp ──►]

HTTP/2 (half-duplex, multiplexed):
  Client: [send req1 ──►] [send req2 ──►]
  Server: [◄── resp1] [◄── resp2]         (still request-response, just overlapping)

WebSocket (full-duplex):
  Client: [send msg1 ──►] [send msg2 ──►] [◄── recv msg3] [send msg4 ──►]
  Server: [◄── recv msg1] [send msg3 ──►] [◄── recv msg2] [send msg5 ──►]
           ^ both sides send whenever they want, independently
```

| Protocol | Model | Server can initiate? | Directions |
|---|---|---|---|
| HTTP/1.1 | Request → Response (sequential) | No | One at a time |
| HTTP/2 | Request → Response (multiplexed) | No (server push is dead) | Multiple pairs overlap |
| WebSocket | Messages (no req/resp pattern) | **Yes** | Both directions, anytime |
| SSE | Server → Client stream | **Yes** (one-way only) | Server → Client only |

The key difference: in HTTP, the server is **always reacting** to client requests. In WebSocket, the server can say "hey, new chat message arrived" without the client asking first.

### WebSocket Lifecycle: HTTP → WebSocket → Frames

WebSocket starts as an HTTP/1.1 request, then **hijacks the TCP connection**:

```
Phase 1: HTTP/1.1 Upgrade Handshake
─────────────────────────────────────
Client → Server:
  GET /chat HTTP/1.1              ← regular HTTP/1.1 request
  Host: example.com
  Upgrade: websocket              ← "please switch to WebSocket"
  Connection: Upgrade
  Sec-WebSocket-Key: dGhlIH...    ← random base64 nonce
  Sec-WebSocket-Version: 13

Server → Client:
  HTTP/1.1 101 Switching Protocols  ← "OK, switching now"
  Upgrade: websocket
  Connection: Upgrade
  Sec-WebSocket-Accept: s3pP...    ← SHA1(client_key + magic_GUID), base64

Phase 2: WebSocket Frames (HTTP is GONE — same TCP socket, new protocol)
─────────────────────────────────────
  Client ←══════ full-duplex ══════► Server
  Either side can send frames at any time.
```

After the `101 Switching Protocols` response, HTTP is gone forever. The same TCP socket now speaks the WebSocket binary frame protocol. The connection stays open indefinitely until either side sends a Close frame.

The `Upgrade` mechanism only exists in HTTP/1.1. HTTP/2 has a different way (`CONNECT` with `:protocol` pseudo-header), but in practice almost all WebSocket connections use the HTTP/1.1 upgrade, even in 2026.

### WebSocket Frame Format

```
Byte 0:  [FIN(1) RSV(3) OPCODE(4)]
Byte 1:  [MASK(1) LENGTH(7)]
         If LENGTH == 126: next 2 bytes = actual length
         If LENGTH == 127: next 8 bytes = actual length
If MASK: 4 bytes masking key, payload XOR'd with mask

Opcodes: 0x1=Text  0x2=Binary  0x8=Close  0x9=Ping  0xA=Pong
```

Key rules from RFC 6455:
- **Client → Server frames MUST be masked** (XOR with random 4-byte key)
- **Server → Client frames are NOT masked**
- This asymmetry prevents cache poisoning attacks on proxies

Use cases: real-time chat, live updates (stock prices, sports), collaborative editing, gaming.

## Long Polling vs SSE vs WebSocket

All three are built **on top of HTTP/1.1** — they're not separate transport protocols:

- **Long Polling**: A normal HTTP/1.1 request where the server delays the response until new data is available. When it responds, the client immediately sends another request. It's just slow HTTP.
- **SSE**: A normal HTTP/1.1 response with `Content-Type: text/event-stream` that **never closes**. The server keeps writing `data: ...\n\n` lines. It's just a long-lived HTTP response.
- **WebSocket**: Starts as an HTTP/1.1 `Upgrade` request → `101 Switching Protocols`, then the TCP socket switches to the WebSocket binary frame protocol. HTTP is gone after the handshake.

```
            HTTP/1.1 request
                  │
    ┌─────────────┼───────────────┬──────────────────┐
    ▼             ▼               ▼                  ▼
Long Polling     SSE          WebSocket           Normal HTTP
(delayed resp)  (never-ending  (101 Upgrade →      (req → resp
 then reconnect) response)     binary frames)       done)
```

| Method | Transport | Direction | Server initiates? | Overhead | Use Case |
|--------|-----------|-----------|-------------------|----------|----------|
| **Long Polling** | HTTP/1.1 request loop | Server → Client | No (client polls) | High (reconnect) | Legacy real-time |
| **SSE** | HTTP/1.1 long-lived response | Server → Client | Yes | Low (~10 bytes/event) | Notifications, feeds |
| **WebSocket** | HTTP/1.1 upgrade → WS frames | Bidirectional | Yes | Very low (2-6 bytes) | Chat, gaming |

## gRPC (Google Remote Procedure Call)

gRPC is an RPC framework that runs over **HTTP/2**. Instead of REST (send JSON over HTTP), you define services in Protocol Buffers (protobuf) and call remote functions as if they were local.

### Why gRPC Over REST?

| | REST (JSON over HTTP) | gRPC (Protobuf over HTTP/2) |
|---|---|---|
| **Serialization** | JSON (text, ~2x larger) | Protobuf (binary, compact) |
| **Schema** | OpenAPI/Swagger (optional) | `.proto` file (mandatory, typed) |
| **Transport** | HTTP/1.1 or HTTP/2 | HTTP/2 only |
| **Code generation** | Manual or codegen tools | Built-in (protoc generates client/server) |
| **Streaming** | Chunked transfer or SSE | 4 modes (see below) |
| **Browser support** | Native | Needs gRPC-Web proxy |
| **Best for** | Public APIs, simple CRUD | Internal microservices, high-throughput |

### How gRPC Works on the Wire

gRPC is HTTP/2 with a specific framing convention:

```
1. Client sends HTTP/2 HEADERS frame:
   :method = POST
   :path = /mypackage.MyService/GetUser        ← service/method name
   content-type = application/grpc
   grpc-encoding = identity (or gzip)

2. Client sends HTTP/2 DATA frame(s):
   [1 byte: compressed flag] [4 bytes: message length] [N bytes: protobuf message]

   Example: [0x00] [0x00 0x00 0x00 0x0A] [protobuf bytes...]
            not compressed   10 bytes      the actual request

3. Server sends HTTP/2 HEADERS + DATA frames (same format)

4. Server sends trailing HEADERS:
   grpc-status = 0 (OK)
   grpc-message = "" (or error description)
```

The key insight: **gRPC is just HTTP/2 + protobuf + a 5-byte message prefix**. There's no new transport protocol — it reuses all of HTTP/2's features (multiplexing, flow control, HPACK).

### Four Streaming Modes

```
1. Unary RPC (normal request-response):
   Client ──[request]──► Server ──[response]──► Client

2. Server streaming:
   Client ──[request]──► Server ──[response1]──►
                                 ──[response2]──►
                                 ──[response3]──► Client

3. Client streaming:
   Client ──[request1]──►
           ──[request2]──►
           ──[request3]──► Server ──[response]──► Client

4. Bidirectional streaming:
   Client ──[msg1]──► Server ──[msg2]──► Client
          ──[msg3]──►        ──[msg4]──►
           (both directions independently, like WebSocket but over HTTP/2)
```

All four modes use the same HTTP/2 stream — they just differ in how many DATA frames each side sends before finishing.

### When to Use gRPC vs REST

- **REST**: Public APIs (browser-friendly), simple CRUD, when human-readable responses matter
- **gRPC**: Internal microservice-to-microservice calls, latency-sensitive systems, when you need streaming, strong typing, or high throughput
- **Both**: Many companies expose REST externally and use gRPC internally (with a gateway that translates)

## TLS (Transport Layer Security)

TLS encrypts the connection between client and server. Without TLS, anyone on the network (WiFi, ISP, corporate proxy) can read your passwords, tokens, and data in plaintext.

### What TLS Provides

| Property | What It Does | How |
|---|---|---|
| **Encryption** | Nobody can read the traffic | AES-256-GCM or ChaCha20-Poly1305 |
| **Authentication** | Server proves it's really google.com | X.509 certificate signed by a CA |
| **Integrity** | Nobody can tamper with the data | HMAC on every record |

### TLS Handshake (TLS 1.3)

```
Client                                  Server
  │                                       │
  │── ClientHello ───────────────────────►│
  │   • Supported cipher suites            │
  │   • TLS version (1.3)                  │
  │   • Random nonce                       │
  │   • SNI: "api.example.com"             │  ← which host?
  │   • Key share (Diffie-Hellman)         │
  │                                       │
  │◄── ServerHello + Certificate ─────────│
  │   • Chosen cipher suite               │
  │   • Server's key share                │
  │   • X.509 certificate chain           │  ← proves identity
  │   • Finished                          │
  │                                       │
  │── Finished ──────────────────────────►│  (1 RTT total for TLS 1.3!)
  │                                       │
  │═══ Encrypted Application Data ═══════│  ← all HTTP traffic here
```

**TLS 1.3 vs 1.2**: TLS 1.3 completes in **1 RTT** (vs 2 RTT for 1.2), removed all weak ciphers, and supports **0-RTT resumption** (send data immediately on reconnection using a pre-shared key).

### Key Concepts

- **Certificate (X.509)**: A file containing the server's public key + domain name, signed by a CA. The client verifies this signature to confirm the server is who it claims to be.
- **CA (Certificate Authority)**: A trusted third party that signs certificates. Your OS/browser ships with ~100 trusted root CAs (Let's Encrypt, DigiCert, etc.).
- **Certificate Chain**: Server cert → Intermediate CA → Root CA. Client walks the chain until it finds a trusted root.
- **SNI (Server Name Indication)**: The client sends the hostname in the ClientHello (plaintext). This lets one IP host multiple HTTPS domains. Without SNI, the server doesn't know which certificate to present.
- **ALPN (Application-Layer Protocol Negotiation)**: During the TLS handshake, the client says "I support h2, http/1.1" and the server picks one. This is how HTTP/2 is negotiated — no extra round trips.
- **0-RTT (TLS 1.3)**: On a resumed connection, the client can send data in the very first message using a pre-shared key from a previous session. Zero round-trip latency. Trade-off: vulnerable to replay attacks, so only safe for idempotent requests (GET, not POST).
- **mTLS (Mutual TLS)**: Both client AND server present certificates. Used in microservice-to-microservice auth (e.g., Istio service mesh). The server verifies the client's identity too.

### TLS Version Comparison

| Version | Handshake | Cipher Suites | 0-RTT | Status (2026) |
|---|---|---|---|---|
| TLS 1.0 | 2 RTT | Weak (RC4, MD5) | No | **DEAD** — removed from all browsers |
| TLS 1.1 | 2 RTT | Weak | No | **DEAD** — removed from all browsers |
| TLS 1.2 | 2 RTT | Mixed (some weak) | No | **Legacy** — still works but 1.3 preferred |
| TLS 1.3 | **1 RTT** | **Strong only** (AES-GCM, ChaCha20) | **Yes** | **Current** — used by default everywhere |

### Where TLS Fits in the Stack

```
Your code:   GET /api/users
     ↕
HTTP layer:  HTTP/1.1 text  or  HTTP/2 binary  or  HTTP/3
     ↕                            ↕                   ↕
TLS layer:   TLS 1.3 encryption   TLS 1.3           Built into QUIC
     ↕                            ↕                   ↕
Transport:   TCP                  TCP                 UDP (QUIC)
```

HTTPS = HTTP + TLS. It's not a separate protocol — it's HTTP running inside a TLS tunnel. The same is true for WSS (WebSocket Secure) = WebSocket + TLS.

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
