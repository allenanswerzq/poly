# Operating Systems — What Principal Engineers Must Know

## Why This Matters

Every performance problem you debug, every system you design, and every scaling decision you make sits on top of the OS. When an interviewer asks "why is this slow?" or "how would you handle 100K connections?", they're probing your OS knowledge.

## 1. Processes vs Threads

```
Process:                          Thread:
┌─────────────────────┐          ┌─────────────────────┐
│ Own memory space     │          │ Shared memory space  │
│ Own file descriptors │          │ Shared file descs    │
│ Expensive to create  │          │ Cheap to create      │
│ Crash = isolated     │          │ Crash = kills all    │
│ IPC needed to talk   │          │ Just read shared mem │
└─────────────────────┘          └─────────────────────┘

PostgreSQL: 1 process per connection      (isolation, ~10MB each)
Nginx:      1 process, many threads       (efficiency)
Node.js:    1 process, 1 thread, async IO (event loop)
Go:         1 process, goroutines (M:N)   (lightweight green threads)
Rust/Tokio: 1 process, async tasks on thread pool (like Go)
```

### Fork (how PostgreSQL creates workers)
```
Parent process → fork() → child process (copy of parent)
  - Copy-on-write: child shares parent's memory pages
  - Only copied when child writes to a page
  - This is why PostgreSQL can fork quickly even with large shared buffers
```

## 2. Memory Management

### Virtual Memory
```
Every process thinks it has the entire address space:

Process A sees:          Physical RAM:         Disk (swap):
┌──────────────┐        ┌──────────────┐     ┌──────────────┐
│ 0x0000: code │───────►│ Page frame 5 │     │              │
│ 0x1000: heap │───────►│ Page frame 12│     │              │
│ 0x2000: data │────┐   │ Page frame 2 │     │ Page swapped │
└──────────────┘    └──►│              │     │   out here   │
                        └──────────────┘     └──────────────┘

Page fault: access swapped-out page → OS loads from disk → SLOW (ms vs ns)
```

### Key Numbers
| Operation | Latency |
|-----------|---------|
| L1 cache hit | 1 ns |
| L2 cache hit | 4 ns |
| L3 cache hit | 12 ns |
| RAM access | 100 ns |
| Page fault (SSD swap) | 100 µs |
| Page fault (HDD swap) | 10 ms |

### Why It Matters for System Design
- **Redis**: keeps everything in RAM — page faults would destroy latency
- **mmap**: map files into virtual memory, OS manages page loading
- **OOM Killer**: Linux kills processes when RAM is exhausted (know this!)
- **Huge Pages**: 2MB instead of 4KB pages, reduces TLB misses for large datasets

## 3. File I/O

### The I/O Stack
```
Application: write("hello")
     │
     ▼
OS Page Cache (RAM buffer)    ← write returns HERE (fast)
     │
     ▼ (eventually, async)
File System (ext4, xfs)
     │
     ▼
Block Device Driver
     │
     ▼
Disk (SSD/HDD)               ← data actually persists HERE (slow)
```

### Buffered vs Direct I/O
```
Buffered (default):
  write() → OS page cache → disk (async)
  + Fast (returns immediately)
  - Data can be lost on crash (not on disk yet)

Direct I/O (O_DIRECT):
  write() → bypass page cache → disk (sync)
  + Data on disk immediately
  - Slower
  Used by: databases that manage their own caching (PostgreSQL, MySQL)

fsync():
  Force page cache → disk
  This is what makes database commits durable
```

### Epoll / Kqueue / io_uring
```
The problem: how to handle 100K network connections?

Per-thread model (Apache):
  1 thread per connection → 100K threads → too much memory

Event-driven (Nginx, Node.js, Tokio):
  1 thread watches 100K file descriptors with epoll/kqueue
  epoll_wait() → "these 50 sockets have data ready" → process them
  No thread-per-connection overhead

Platform-specific mechanisms:

  select() (1983, all platforms):
    Pass a list of FDs, kernel scans ALL of them each call → O(n)
    Limited to 1024 FDs. Too slow for modern workloads.

  poll() (1986, all platforms):
    Like select but no FD limit. Still O(n) per call.

  epoll (Linux 2.5.44+):
    Kernel maintains the FD set. Only returns READY ones → O(ready)
    Three calls: epoll_create, epoll_ctl(add/remove), epoll_wait
    Used by: Nginx, Tokio, Node.js (on Linux)

  kqueue (BSD/macOS):
    Same idea as epoll, but for BSD-family OSes (macOS, FreeBSD)
    One syscall: kevent() — both register interest AND poll for events
    More general than epoll: works for sockets, files, signals, timers
    Used by: Nginx, Tokio, Node.js (on macOS)

  io_uring (Linux 5.1+):
    True async I/O with shared ring buffers between user and kernel
    Submit requests + reap completions with ZERO syscalls (shared memory)
    Handles network AND disk I/O (epoll is network only)
    Used by: modern databases, Tokio (experimental)

  ┌──────────┬──────────┬───────────┬──────────────┬───────────────┐
  │          │ Platform │ Scaling   │ Disk I/O?    │ Syscalls/op   │
  ├──────────┼──────────┼───────────┼──────────────┼───────────────┤
  │ select   │ All      │ O(n)      │ No           │ 1 per poll    │
  │ poll     │ All      │ O(n)      │ No           │ 1 per poll    │
  │ epoll    │ Linux    │ O(ready)  │ No           │ 1 per poll    │
  │ kqueue   │ BSD/Mac  │ O(ready)  │ Partial      │ 1 per poll    │
  │ io_uring │ Linux    │ O(ready)  │ Yes          │ 0 (ring buf)  │
  └──────────┴──────────┴───────────┴──────────────┴───────────────┘

Cross-platform libraries abstract these away:
  libuv (Node.js):        epoll on Linux, kqueue on macOS, IOCP on Windows
  mio (Rust/Tokio):       epoll on Linux, kqueue on macOS
  libevent/libev:         epoll/kqueue/select depending on OS
```

## 4. Networking (OS Level)

### TCP Connection Lifecycle
```
Client                          Server
  │ SYN ────────────────────────► │  ← SYN queue (backlog)
  │ ◄──────────────────── SYN-ACK │
  │ ACK ────────────────────────► │  ← Accept queue → application
  │                                │
  │ ◄──── data ────►              │  ← Established
  │                                │
  │ FIN ────────────────────────► │  ← TIME_WAIT (2 × MSL)
```

### Connection Limits
```
ulimit -n              → max open file descriptors (default: 1024, set to 65535+)
net.core.somaxconn     → max accept queue (default: 128, set to 4096+)
net.ipv4.tcp_max_syn_backlog → max SYN queue

Running out of ports?
  Client-side: 65535 - 1024 = ~64K ports per destination IP
  Solution: connection pooling (don't open new connection per request)
```

### Incoming vs Outgoing Connections — The Full Picture

```
Incoming (server-side):  Users → Your App
  Problem:  100K users connecting simultaneously
  Solution: epoll/kqueue event loop + load balancer

Outgoing (client-side):  Your App → Database / Redis / APIs
  Problem:  too many connections opened, port exhaustion, handshake cost
  Solution: connection pool (reuse N pre-opened connections)
```

```
How the server handles incoming:

  Thread-per-connection (Apache):
    100K connections → 100K threads → ~1TB RAM → DIES

  Event loop (Nginx, Node.js, Tokio):
    100K connections → 1 thread + epoll → ~1GB RAM → FINE

  Async tasks (Go, Rust/Tokio):
    100K connections → 8 OS threads + 100K tasks → ~200MB → GREAT
```

```
How the app manages outgoing (connection pool):

  Without pool:
    10K req/s → 10K new TCP connections/s → port exhaustion in 6 seconds

  With pool (20 connections):
    10K req/s → 20 reused connections → each handles ~500 queries/s
    Ports used: 20 (forever). No exhaustion.

  ┌──────────────────────────────────────────────┐
  │  Connection Pool (20 connections to DB)       │
  │  ┌────┐ ┌────┐ ┌────┐ ┌────┐ ... ┌────┐    │
  │  │conn│ │conn│ │conn│ │conn│     │conn│    │
  │  │ 1  │ │ 2  │ │ 3  │ │ 4  │     │ 20 │    │
  │  └────┘ └────┘ └────┘ └────┘     └────┘    │
  │   busy   busy   idle   idle       idle      │
  └──────────────────────────────────────────────┘
  Request arrives → borrow idle conn → query → return to pool
  All busy? → wait in queue (or timeout with error)
```

```
                    Incoming (server-side)              Outgoing (client-side)
                    Users → Your App                    Your App → Database
┌─────────────────────────────────────────┬────────────────────────────────────┐
│ Problem     │ Too many simultaneous     │ Port exhaustion, handshake cost,  │
│             │ clients connecting        │ overloading backend               │
├─────────────┼───────────────────────────┼────────────────────────────────────┤
│ Solution    │ epoll/kqueue event loop   │ Connection pool (reuse N conns)   │
│             │ + load balancer           │                                    │
├─────────────┼───────────────────────────┼────────────────────────────────────┤
│ Who waits?  │ OS queues (SYN, accept)   │ App waits for pool slot           │
├─────────────┼───────────────────────────┼────────────────────────────────────┤
│ Key limit   │ File descriptors (ulimit) │ Ephemeral ports (~64K)            │
├─────────────┼───────────────────────────┼────────────────────────────────────┤
│ Scaling     │ More servers behind LB    │ Bigger pool (don't overload DB)   │
├─────────────┼───────────────────────────┼────────────────────────────────────┤
│ Typical     │ Nginx: 10K+ per worker    │ DB pool: 10-200 connections       │
│ numbers     │ Tokio: 100K+ per process  │ HTTP pool: 50-200 connections     │
└─────────────┴───────────────────────────┴────────────────────────────────────┘
```

### Zero-Copy (sendfile)
```
Without zero-copy:
  disk → kernel buffer → user buffer → kernel buffer → network
  (4 copies, 2 context switches)

With sendfile():
  disk → kernel buffer → network buffer
  (2 copies, 0 user-space copies)

Used by: Nginx (serving static files), Kafka (sending log segments)
```

## 5. Scheduling & Context Switching

```
Context switch:
  Save registers, stack pointer, program counter
  Flush TLB (if switching processes)
  Load new process state
  Cost: ~1-10 µs

1000 threads × frequent switching = significant overhead
This is why async runtimes (Tokio, Go scheduler) minimize OS-level switches
by using M:N threading (many tasks, few OS threads)
```

## 6. Containers & cgroups

A container is NOT a VM. It's just a **regular Linux process** with two kernel features applied:

```
Namespaces = what can the process SEE?   (isolation)
cgroups    = how much can it USE?        (resource limits)

Together they create the illusion of a separate machine.
```

### Namespaces — Isolation (What the process sees)

Each namespace type hides a different part of the system:

```
┌─────────────────────────────────────────────────────────────────────┐
│ Namespace    │ Isolates                  │ What container sees       │
├──────────────┼───────────────────────────┼───────────────────────────┤
│ PID          │ Process IDs               │ Own PID tree (PID 1)      │
│ Mount (mnt)  │ Filesystem mounts         │ Own / with bind mounts    │
│ Network (net)│ Network stack             │ Own IP, ports, interfaces │
│ UTS          │ Hostname                  │ Own hostname              │
│ IPC          │ Shared memory, semaphores │ Own IPC resources         │
│ User         │ User/group IDs            │ UID 0 (root) inside,     │
│              │                           │ unprivileged outside      │
│ Cgroup       │ Cgroup root view          │ Sees only its own cgroup  │
└──────────────┴───────────────────────────┴───────────────────────────┘
```

```
Without namespaces:
  Process sees ALL other processes, ALL mount points, host's real IP

With namespaces:
  Process sees only its OWN process tree, mounts, network
  Thinks it's running on its own machine

  Host:                          Container sees:
  ┌─────────────────────┐       ┌─────────────────────┐
  │ PID 1: systemd      │       │ PID 1: my-app       │ ← thinks it's PID 1
  │ PID 42: sshd        │       │ PID 2: worker       │
  │ PID 100: my-app ◄───┼───┐   └─────────────────────┘
  │ PID 101: worker  ◄──┼───┘     (can't see systemd, sshd, etc.)
  └─────────────────────┘
```

### Mount namespace — how Docker volumes work

```
Container gets its own filesystem view via mount namespace:

  Host filesystem:           Container mount namespace:
  /                          /                     ← overlayfs (image layers)
  ├── home/yibai/data/       ├── app/              ← from image layer
  ├── var/lib/docker/        ├── data/ ◄───────────── bind mount from host
  └── ...                    ├── tmp/              ← tmpfs (RAM, ephemeral)
                             └── etc/hosts ◄──────── bind mount single file

  docker run -v /home/yibai/data:/data  myimage
  ↓
  Under the hood:
    1. Create new mount namespace (unshare(CLONE_NEWNS))
    2. Mount overlayfs for root (read-only image layers + writable layer)
    3. mount --bind /home/yibai/data /data  (folder bind mount)
    4. mount -t tmpfs tmpfs /tmp            (RAM-backed temp)
    5. mount --bind /etc/resolv.conf ...    (DNS config injection)
```

### cgroups — Resource Limits (How much the process can use)

Namespaces don't limit resource usage — a namespaced process could still eat 100%
CPU and all RAM. **cgroups** enforce the limits:

```
cgroups (control groups):
  ├── cpu
  │     cpu.max = "200000 100000"    → max 2 cores (200ms per 100ms period)
  │     cpu.weight = 100             → relative CPU share
  ├── memory
  │     memory.max = 4294967296      → 4 GB hard limit
  │     memory.high = 3221225472     → 3 GB (throttle before OOM)
  │     memory.swap.max = 0          → no swap (common in production)
  ├── io
  │     io.max = "8:0 rbps=104857600"  → 100 MB/s read from /dev/sda
  └── pids
        pids.max = 1000              → max 1000 processes (fork bomb protection)

Enforcement:
  CPU exceeded?    → process gets throttled (scheduled less)
  Memory exceeded? → OOM killer kills a process in the cgroup
  PIDs exceeded?   → fork() returns EAGAIN
```

### How they connect — Docker puts them together

```
docker run --cpus=2 --memory=4g -v /data:/data -p 8080:80 myapp

  What Docker does:
  ┌─────────────────────────────────────────────────────────────────┐
  │ 1. Fork a new process                                          │
  │                                                                 │
  │ 2. Apply NAMESPACES (isolation):                                │
  │    ├── PID namespace    → app sees itself as PID 1              │
  │    ├── Mount namespace  → overlayfs root + bind mount /data     │
  │    ├── Network namespace→ own eth0, own IP, port 80 mapped      │
  │    ├── UTS namespace    → own hostname (container ID)           │
  │    └── User namespace   → root inside, nobody outside           │
  │                                                                 │
  │ 3. Apply CGROUPS (limits):                                      │
  │    ├── cpu.max = 2 cores                                        │
  │    ├── memory.max = 4GB                                         │
  │    └── pids.max = 4096                                          │
  │                                                                 │
  │ 4. exec() the application binary                                │
  │                                                                 │
  │ Result: process thinks it's on its own machine with 2 CPU, 4GB  │
  │         Actually just a regular process with kernel restrictions │
  └─────────────────────────────────────────────────────────────────┘
```

### Container vs VM

```
┌──────────────────┬───────────────────────┬───────────────────────┐
│                  │ Container             │ VM                    │
├──────────────────┼───────────────────────┼───────────────────────┤
│ Isolation via    │ Namespaces + cgroups  │ Hypervisor + own kernel│
│ Kernel           │ Shared with host      │ Separate guest kernel │
│ Startup time     │ ~100ms               │ ~30-60 seconds        │
│ Memory overhead  │ ~10 MB               │ ~200+ MB (kernel+OS)  │
│ Isolation level  │ Process-level         │ Hardware-level        │
│ Security         │ Weaker (shared kernel)│ Stronger (separate)   │
│ Density          │ 100s per host         │ 10s per host          │
│ Use case         │ Microservices, CI/CD  │ Multi-tenant, legacy  │
└──────────────────┴───────────────────────┴───────────────────────┘

Firecracker (AWS Lambda): micro-VM — VM-level isolation,
  container-like speed (~125ms boot). Best of both worlds.
```

### cgroups v1 vs v2

```
cgroups v1 (legacy):
  Each resource type is a separate hierarchy:
    /sys/fs/cgroup/cpu/docker/container-abc/
    /sys/fs/cgroup/memory/docker/container-abc/
    /sys/fs/cgroup/blkio/docker/container-abc/
  Problem: a process can be in different cgroups for CPU vs memory.
  Complex, hard to manage.

cgroups v2 (modern, default since ~2022):
  Unified single hierarchy:
    /sys/fs/cgroup/docker/container-abc/
      ├── cpu.max
      ├── memory.max
      ├── io.max
      └── pids.max
  Simpler, all limits in one place.
  Required by: newer Docker, Kubernetes, systemd.
```

## Interview Talking Points

| Question | What to Say |
|----------|-------------|
| "Why is this slow?" | Check: page faults? disk I/O? context switches? network? |
| "Handle 100K connections?" | epoll/kqueue event loop, not thread-per-connection |
| "Why does Redis restart slowly?" | Loading dataset from disk back into RAM, page faults |
| "Docker vs VM?" | Containers share kernel (fast, lightweight), VMs have full kernel (isolated, slower) |
| "Why connection pooling?" | Avoid TCP handshake + port exhaustion, reuse established connections |
| "fsync performance?" | SSD: 0.1-2ms, HDD: 5-15ms. Batch commits to minimize fsyncs |
