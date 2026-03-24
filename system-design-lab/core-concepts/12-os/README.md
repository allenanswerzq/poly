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
  1 thread watches 100K file descriptors with epoll()
  epoll_wait() → "these 50 sockets have data ready" → process them
  No thread-per-connection overhead

io_uring (Linux 5.1+):
  Async I/O with shared ring buffers between user and kernel
  Even less overhead than epoll for high-throughput I/O
  Used by: modern databases, Tokio (experimental)
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

```
Docker container = process with resource limits

cgroups (control groups):
  ├── CPU: max 2 cores for this process
  ├── Memory: max 4GB (OOM if exceeded)
  ├── I/O: max 100MB/s disk bandwidth
  └── Network: bandwidth limits

Namespaces:
  ├── PID: container sees its own process tree (PID 1)
  ├── Network: own IP address, ports
  ├── Mount: own filesystem view
  └── User: own root user (unprivileged on host)

Container ≠ VM:
  VM: full OS kernel + hypervisor overhead
  Container: shared kernel, just namespace isolation, near-zero overhead
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
