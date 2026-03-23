# Gateways — API Gateway, Reverse Proxy, and Service Mesh

## What Is a Gateway?

A gateway sits **between clients and your backend services**. Every request from the outside world goes through the gateway before reaching your services. It's the front door of your system.

```
                        ┌──────────────┐
    Internet            │              │           Internal Network
                        │              │
  Mobile App ──────────►│              │──────► User Service
  Web Browser ─────────►│   GATEWAY    │──────► Order Service
  Partner API ─────────►│              │──────► Payment Service
  IoT Device ──────────►│              │──────► Notification Service
                        │              │
                        └──────────────┘

  Single entry point.   Handles cross-cutting concerns so your
                        services don't have to.
```

## Why Do You Need a Gateway?

Without a gateway, every service must handle:
- Authentication (validate JWT tokens)
- Rate limiting (prevent abuse)
- SSL termination (TLS certificates)
- Logging and monitoring
- CORS headers
- Request routing

That's duplicated logic in every service. A gateway handles it once, in one place.

## Types of Gateways

### 1. Reverse Proxy

The simplest gateway. Receives requests, forwards them to backend services, returns the response. The client never talks to the backend directly.

```
Client ──► Reverse Proxy ──► Backend Server
       ◄──               ◄──

Key features:
• SSL termination (HTTPS outside, HTTP inside)
• Load balancing
• Caching static content
• Hiding backend topology

Examples: nginx, Caddy, HAProxy, Envoy
```

### 2. API Gateway

A reverse proxy with **application-level intelligence**. It understands your API — routes, authentication, rate limits, request/response transformation.

```
Client ──► API Gateway ──► /users  → User Service
                       ──► /orders → Order Service
                       ──► /pay    → Payment Service

Additional features over reverse proxy:
• Path-based routing (/users → service A, /orders → service B)
• Authentication & authorization (validate JWT, check permissions)
• Rate limiting (100 req/min per API key)
• Request/response transformation (add headers, filter fields)
• API versioning (/v1/users → old service, /v2/users → new service)
• Circuit breaking (stop sending to failing services)
• Request aggregation (fan-out to multiple services, combine responses)

Examples: Kong, AWS API Gateway, Apigee, Traefik
```

### 3. Service Mesh Sidecar

For **internal** service-to-service communication. Instead of a central gateway, every service gets a sidecar proxy that handles networking concerns.

```
┌─────────────────────┐     ┌─────────────────────┐
│  User Service       │     │  Order Service       │
│  ┌───────────────┐  │     │  ┌───────────────┐  │
│  │  App Code     │  │     │  │  App Code     │  │
│  └──────┬────────┘  │     │  └──────┬────────┘  │
│         │ localhost  │     │         │ localhost  │
│  ┌──────▼────────┐  │     │  ┌──────▼────────┐  │
│  │  Envoy Proxy  │──┼─────┼──│  Envoy Proxy  │  │
│  │  (sidecar)    │  │     │  │  (sidecar)    │  │
│  └───────────────┘  │     │  └───────────────┘  │
└─────────────────────┘     └─────────────────────┘

• mTLS between all services (automatic encryption)
• Retry, timeout, circuit breaking per-service
• Distributed tracing headers (propagate trace IDs)
• No code changes — the sidecar handles everything

Examples: Istio (Envoy), Linkerd, Consul Connect
```

## Gateway Patterns

### Pattern 1: Reverse Proxy with Path-Based Routing

Route different URL paths to different backend services:

```
/api/users/*    → http://user-service:8080
/api/orders/*   → http://order-service:8080
/api/payments/* → http://payment-service:8080
/static/*       → http://cdn-origin:8080
```

The client sees one domain (`api.example.com`), but requests go to different services.

### Pattern 2: Authentication at the Gateway

Validate tokens **once** at the gateway, then forward authenticated requests:

```
Client ──[JWT token]──► Gateway ──[validate token]──► Accept/Reject
                                   │
                                   │ valid → add X-User-ID header
                                   ▼
                            Backend Service (trusts X-User-ID)
```

Benefits:
- Services don't need JWT libraries or auth logic
- One place to rotate keys, add OAuth providers
- Internal traffic is already authenticated

### Pattern 3: Rate Limiting at the Gateway

```
Client A ──► Gateway ──► [check: 95/100 requests this minute] ──► Allow → Backend
Client B ──► Gateway ──► [check: 101/100 requests this minute] ──► 429 Too Many Requests
```

Rate limiting is best at the gateway because:
- One place to enforce limits across all services
- Can rate-limit by API key, IP, user, or path
- Protects all backends uniformly

#### Rate Limiting Algorithms Compared

**Token Bucket (what we use — and what most production systems use)**

```
Bucket for "client-A" (max 5 tokens, refill 2/sec):

  Time 0:  [●●●●●]  5 tokens (full)
  Req 1:   [●●●● ]  4 tokens
  Req 2:   [●●●  ]  3 tokens
  Req 3:   [●●   ]  2 tokens
  Req 4:   [●    ]  1 token
  Req 5:   [     ]  0 tokens → bucket empty
  Req 6:   REJECTED (429 Too Many Requests)

  ...1 second passes, 2 tokens refill...

  Req 7:   [●    ]  1 token left → allowed
```

Allows bursts (spend saved-up tokens instantly), enforces sustained rate (refill caps long-term throughput), O(1) memory per client.

**Fixed Window Counter** — count requests per time window (e.g., 100/min)

```
Window: [00:00 - 01:00] limit = 100

00:00──────────────────01:00──────────────────02:00
       80 requests            20 requests
                    ↑ BUT: 50 at 00:59 + 50 at 01:01 = 100 in 2 seconds!
```

Problem: burst at window boundaries. A client can send 100 requests at 00:59 and 100 more at 01:01 — 200 requests in 2 seconds while "respecting" the limit.

**Sliding Window Log** — store timestamp of every request, count those within last N seconds

Problem: stores every request timestamp. At 10K req/s that's 600K timestamps per window per client. Memory explodes.

**Leaky Bucket** — requests enter a queue, processed at a fixed drip rate

```
┌─────────┐
│ ● ● ● ● │ → drip out at 2/sec (fixed rate, no bursts)
└─────────┘
```

Problem: no bursts allowed. Legit users who send 3 quick requests have to wait in a queue. Adds latency.

#### Why Token Bucket Wins

| Concern | Token Bucket | Fixed Window | Sliding Log | Leaky Bucket |
|---------|-------------|-------------|-------------|-------------|
| **Allows bursts** | Yes (up to bucket size) | Boundary exploit | No burst control | No (fixed drip) |
| **Steady-rate limit** | Yes (refill rate) | Approximate | Yes | Yes |
| **Memory** | O(1) per client | O(1) per client | O(n) per request | O(1) per client |
| **Latency added** | None (allow/deny) | None | None | Yes (queue wait) |
| **Boundary issues** | None | Yes (double burst) | None | None |

Used by: nginx, AWS API Gateway, Stripe, Cloudflare, Kong.

### Pattern 4: Request Aggregation (BFF — Backend for Frontend)

Mobile app needs data from 3 services. Instead of 3 round-trips:

```
Without gateway:
  Mobile ──► User Service    (RTT 1)
  Mobile ──► Order Service   (RTT 2)
  Mobile ──► Payment Service (RTT 3)
  Total: 3 round trips on slow mobile network

With gateway aggregation:
  Mobile ──► Gateway ──► User Service    ┐
                     ──► Order Service   ├── parallel, internal network (fast)
                     ──► Payment Service ┘
  Mobile ◄── Gateway (combined response)
  Total: 1 round trip over mobile network
```

This is the **Backend for Frontend (BFF)** pattern. The gateway acts as a custom API layer for each client type.

### Pattern 5: Circuit Breaking

Stop sending requests to a failing service:

```
Gateway → Order Service (failing)
  Attempt 1: timeout (500ms)
  Attempt 2: timeout (500ms)
  Attempt 3: error
  → Circuit OPEN: stop sending for 30 seconds
  → Return cached response or 503

After 30 seconds:
  → Circuit HALF-OPEN: try one request
  → If success: circuit CLOSED, resume traffic
  → If fail: circuit stays OPEN for another 30 seconds
```

## Real-World Gateway Architecture

```
                        ┌─────────────────────────────────────────────┐
                        │              API Gateway                     │
                        │                                              │
  Client ──► DNS ──►    │  1. SSL Termination (TLS 1.3)               │
                        │  2. Rate Limiting (token bucket)             │
                        │  3. Authentication (JWT validation)          │
                        │  4. Routing (/users → service A)             │
                        │  5. Load Balancing (round-robin)             │
                        │  6. Circuit Breaking                         │
                        │  7. Request Logging                          │
                        │  8. Metrics (Prometheus)                     │
                        │                                              │
                        └────────┬───────────┬───────────┬─────────────┘
                                 │           │           │
                                 ▼           ▼           ▼
                           User Service  Order Svc  Payment Svc
                           (3 replicas)  (2 repl)   (2 repl)
```

## Gateway vs Load Balancer vs Reverse Proxy

| | Reverse Proxy | Load Balancer | API Gateway |
|---|---|---|---|
| **Layer** | L7 (HTTP) | L4 (TCP) or L7 | L7 (HTTP) |
| **Routing** | Path/host based | Round-robin, least-conn | Path + auth + rules |
| **Auth** | No | No | Yes (JWT, OAuth, API key) |
| **Rate limit** | Basic | No | Yes (per-user, per-key) |
| **Transform** | Basic headers | No | Full (rewrite body, aggregate) |
| **Caching** | Yes | No | Yes |
| **Examples** | nginx, Caddy | ELB, HAProxy | Kong, AWS API Gateway |

In practice, these overlap. nginx can do load balancing and basic auth. Kong is both a reverse proxy and an API gateway. The distinction is about **how much application-level logic** the gateway handles.

## Implementation

Our demo includes:
1. **Reverse proxy** — forward requests to backend services, path-based routing
2. **Rate limiting** — token bucket rate limiter at the gateway
3. **Authentication** — JWT-like token validation middleware
4. **Circuit breaker** — detect failing backends and stop forwarding
5. **Request aggregation** — fan-out to multiple services, combine responses

Run the demo:
```bash
cargo run --bin gateways
```
