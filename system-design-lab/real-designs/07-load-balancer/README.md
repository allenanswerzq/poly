# Load Balancer Design

## Overview

A load balancer distributes incoming requests across multiple servers to ensure:
- High availability
- Better performance
- Scalability

## Types of Load Balancers

### Layer 4 (Transport Layer)
- Works at TCP/UDP level
- Routes based on IP and port
- Very fast, no content inspection
- Example: AWS NLB

### Layer 7 (Application Layer)
- Works at HTTP/HTTPS level
- Can route based on URL, headers, cookies
- More flexible, can do SSL termination
- Example: AWS ALB, Nginx

```
┌─────────────────────────────────────────────────────────────────┐
│                     L4 vs L7 Load Balancing                      │
│                                                                  │
│  L4 (Transport)                L7 (Application)                  │
│  ┌─────────────┐              ┌─────────────┐                   │
│  │   Client    │              │   Client    │                   │
│  └──────┬──────┘              └──────┬──────┘                   │
│         │ TCP/UDP                    │ HTTP                      │
│         ▼                            ▼                           │
│  ┌─────────────┐              ┌─────────────┐                   │
│  │     LB      │              │     LB      │                   │
│  │ (IP + Port) │              │ (URL, HDR)  │                   │
│  └──────┬──────┘              └──────┬──────┘                   │
│         │                            │                           │
│    ┌────┴────┐                  ┌────┴────┐                     │
│    ▼         ▼                  ▼         ▼                     │
│ Server 1  Server 2          /api→S1   /web→S2                   │
└─────────────────────────────────────────────────────────────────┘
```

## Load Balancing Algorithms

| Algorithm | Description | Best For |
|-----------|-------------|----------|
| **Round Robin** | Rotate through servers | Similar server capacity |
| **Weighted RR** | More requests to powerful servers | Mixed server capacities |
| **Least Connections** | Send to server with fewer active connections | Long-lived connections |
| **IP Hash** | Same client → same server | Session affinity |
| **Random** | Random selection | Simple, stateless |
| **Least Response Time** | Fastest server gets request | Performance-critical |

## Health Checking

```
┌─────────────────────────────────────────────────────────────────┐
│                       Health Checking                            │
│                                                                  │
│  Load Balancer                                                   │
│       │                                                          │
│       │  Every 5 seconds:                                        │
│       │  GET /health → 200 OK?                                   │
│       │                                                          │
│       ├────────────────┬────────────────┐                       │
│       ▼                ▼                ▼                       │
│   Server 1 ✓       Server 2 ✓      Server 3 ✗                   │
│   (healthy)        (healthy)        (removed)                    │
│                                                                  │
│  Types:                                                          │
│  • Active: LB probes servers periodically                        │
│  • Passive: LB observes real traffic failures                    │
└─────────────────────────────────────────────────────────────────┘
```

## Session Persistence (Sticky Sessions)

When state is stored on servers, same client must hit same server:

```
Options:
1. IP Hash - Hash client IP to select server
2. Cookie-based - Insert cookie with server ID
3. URL encoding - Append server ID to URLs
```

## High Availability

Load balancers themselves need to be HA:

```
┌─────────────────────────────────────────────────────────────────┐
│                    HA Load Balancer Setup                        │
│                                                                  │
│     Virtual IP (VIP): 10.0.0.100                                │
│           │                                                      │
│     ┌─────┴─────┐                                               │
│     │  Keepalive │  ◄── Heartbeat between LBs                   │
│     │    (VRRP)  │                                               │
│     └─────┬─────┘                                               │
│           │                                                      │
│     ┌─────┴─────┐                                               │
│     ▼           ▼                                               │
│  ┌─────┐    ┌─────┐                                             │
│  │ LB1 │    │ LB2 │                                             │
│  │(Act)│    │(Sby)│                                             │
│  └──┬──┘    └──┬──┘                                             │
│     │          │                                                 │
│     └────┬─────┘                                                 │
│          │                                                       │
│     ┌────┴────┐                                                 │
│     ▼    ▼    ▼                                                 │
│   S1    S2   S3                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation

Our implementation includes:
1. Round Robin
2. Weighted Round Robin
3. Least Connections
4. IP Hash (sticky sessions)
5. Health checking simulation

Run the demo:
```bash
cargo run --bin load-balancer
```
