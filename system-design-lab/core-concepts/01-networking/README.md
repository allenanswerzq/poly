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

```
┌─────────────────────────────────────────────────────────────────┐
│                    HTTP Evolution                                │
│                                                                  │
│  HTTP/1.1                                                       │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Request 1  ────────────────────►  Response 1            │   │
│  │  Request 2  ────────────────────►  Response 2            │   │
│  │  Request 3  ────────────────────►  Response 3            │   │
│  │  (Sequential, head-of-line blocking)                     │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  HTTP/2 (Binary, multiplexed)                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  Stream 1: ═══  ═══  ═══                                 │   │
│  │  Stream 2:    ═══  ═══  ═══                              │   │
│  │  Stream 3:  ═══  ═══  ═══                                │   │
│  │  (Parallel streams on single connection)                  │   │
│  │  + Header compression (HPACK)                             │   │
│  │  + Server push                                            │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  HTTP/3 (QUIC + UDP)                                            │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  • Built on QUIC (UDP-based)                              │   │
│  │  • No TCP head-of-line blocking                           │   │
│  │  • Faster connection establishment (0-RTT)                 │   │
│  │  • Built-in encryption                                    │   │
│  │  • Better mobile performance                               │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

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
