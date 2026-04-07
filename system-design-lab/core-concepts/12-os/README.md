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

### IPC — Inter-Process Communication

Threads share memory, so they just read/write shared variables (with locks).
Processes have **separate address spaces** — they CANNOT read each other's memory.
IPC is every mechanism the OS provides for processes to exchange data.

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Mechanism      │ Speed      │ How it works         │ Used by            │
├────────────────┼────────────┼──────────────────────┼────────────────────┤
│ Pipe           │ Fast       │ Byte stream, 1-way   │ Shell (cmd1 | cmd2)│
│ Unix socket    │ Fast       │ Bidirectional stream  │ Docker, Nginx, DBs │
│ TCP socket     │ Medium     │ Bidirectional, network│ Microservices, RPC │
│ Shared memory  │ Fastest    │ Direct memory access  │ PostgreSQL, Chrome │
│ Memory-mapped  │ Fastest    │ mmap same file/region │ Database engines   │
│ Signal         │ Instant    │ Notification only     │ SIGTERM, SIGKILL   │
│ Message queue  │ Medium     │ Structured messages   │ POSIX mq, System V │
│ Eventfd        │ Fast       │ Counter notification  │ KVM, io_uring      │
└────────────────┴────────────┴──────────────────────┴────────────────────┘
```

**Pipes — the simplest IPC:**

```
Unidirectional byte stream. Created with pipe() syscall.
  Returns two file descriptors: fd[0] (read end), fd[1] (write end).

  Parent process                   Child process
  ┌──────────────┐                ┌──────────────┐
  │ write(fd[1], │───── pipe ────►│ read(fd[0],  │
  │   "hello")   │  kernel buffer │   buf, 5)    │
  └──────────────┘   (64KB default)└──────────────┘

  Shell pipes are exactly this:
    ls -la | grep ".rs" | wc -l
    ↓
    3 processes connected by 2 pipes:
      ls → pipe → grep → pipe → wc

  Named pipe (FIFO):
    mkfifo /tmp/my_pipe
    Process A: echo "data" > /tmp/my_pipe
    Process B: cat /tmp/my_pipe
    Persists in filesystem. Any process can open it by name.
    Used by: some legacy inter-service communication.

  Limitations:
    - Unidirectional (need 2 pipes for bidirectional)
    - Only between related processes (unless named pipe)
    - No message boundaries — just raw bytes
    - Pipe buffer full (64KB) → writer blocks
```

**Unix Domain Sockets — the workhorse of local IPC:**

```
Like a TCP socket, but for processes on the SAME machine.
No network stack overhead (no TCP/IP headers, no checksums).

  Two types:
    SOCK_STREAM:  like TCP — reliable byte stream (most common)
    SOCK_DGRAM:   like UDP — message boundaries, no connection

  Server (bind + listen):
    let listener = UnixListener::bind("/var/run/myapp.sock")?;
    for stream in listener.incoming() {
        handle(stream?);
    }

  Client (connect):
    let stream = UnixStream::connect("/var/run/myapp.sock")?;
    stream.write_all(b"hello")?;

  Performance vs TCP loopback (localhost:port):
    Unix socket: ~2 µs round-trip, zero-copy possible
    TCP loopback: ~10 µs round-trip, full TCP/IP stack
    → Unix sockets are ~5x faster for local communication

  Who uses them:
    Docker:       /var/run/docker.sock   (CLI ↔ daemon)
    PostgreSQL:   /var/run/postgresql/.s.PGSQL.5432
    MySQL:        /var/run/mysqld/mysqld.sock
    Nginx:        upstream php { server unix:/run/php-fpm.sock; }
    Redis:        unixsocket /var/run/redis/redis.sock
    X11/Wayland:  display server ↔ GUI apps
    systemd:      socket activation (pass fd to service)

  Bonus feature — fd passing:
    Unix sockets can send FILE DESCRIPTORS between processes!
    Process A opens a file, sends the fd to Process B via the socket.
    Process B can now read/write that file. No file path needed.

    Used by:
      - systemd socket activation (systemd opens port 80, passes fd to Nginx)
      - Container runtimes (pass device fds to containers)
      - Privilege separation (privileged process opens file, passes to unprivileged)
```

**Shared Memory — fastest IPC, but most dangerous:**

```
Two processes map the SAME physical pages into their address spaces.
No copying at all — both processes read/write the same bytes.

  Process A                    Physical RAM              Process B
  ┌──────────────┐           ┌──────────────┐          ┌──────────────┐
  │ 0x7000: ─────┼──────────►│ Page frame 42│◄─────────┼────── :0x9000│
  │ (mapped)     │           │ shared data   │          │     (mapped) │
  └──────────────┘           └──────────────┘          └──────────────┘

POSIX shared memory (modern, preferred):
  // Writer
  int fd = shm_open("/my_shm", O_CREAT | O_RDWR, 0644);
  ftruncate(fd, 4096);
  void* ptr = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
  memcpy(ptr, data, len);

  // Reader (different process)
  int fd = shm_open("/my_shm", O_RDONLY, 0);
  void* ptr = mmap(NULL, 4096, PROT_READ, MAP_SHARED, fd, 0);
  // ptr points to the SAME physical memory

  Lives under: /dev/shm/ (tmpfs mount)
  $ ls /dev/shm/
  my_shm    pulse-shm-12345    ...

Who uses it:
  PostgreSQL:  shared_buffers (default 128MB, often 8-32GB)
               → ALL postgres worker processes share the page cache
               → This is why PostgreSQL uses processes, not threads:
                 crash isolation + shared memory for the buffer pool

  Chrome:      renderer processes share bitmaps with browser process

  CUDA/GPU:    host-device shared memory for zero-copy transfers

  Database:    shared buffer pools, WAL buffers

The danger:
  Shared memory has NO built-in synchronization.
  Two processes writing simultaneously → data corruption.
  YOU must add synchronization (semaphores, mutexes in shared mem).

  This is why higher-level abstractions (sockets, channels) are preferred.
  Shared memory is for when you NEED the performance.
```

**Signals — lightweight notifications:**

```
Signals are async notifications sent to a process. No data payload.

  Common signals:
  ┌──────────┬────────┬────────────────────────────────────────────┐
  │ Signal   │ Number │ Purpose                                    │
  ├──────────┼────────┼────────────────────────────────────────────┤
  │ SIGTERM  │ 15     │ "Please shut down." Can be caught/handled. │
  │ SIGKILL  │ 9      │ "Die NOW." Cannot be caught. Kernel kills. │
  │ SIGINT   │ 2      │ Ctrl+C from terminal.                      │
  │ SIGHUP   │ 1      │ "Reload config." Nginx, HAProxy use this. │
  │ SIGUSR1  │ 10     │ User-defined. Log rotation, debug dump.    │
  │ SIGCHLD  │ 17     │ Child process died. Parent must wait().    │
  │ SIGPIPE  │ 13     │ Write to broken pipe. Default: kill.       │
  │ SIGSTOP  │ 19     │ Pause process. Cannot be caught.           │
  │ SIGCONT  │ 18     │ Resume paused process.                     │
  └──────────┴────────┴────────────────────────────────────────────┘

  kill -TERM <pid>     # polite shutdown (SIGTERM)
  kill -9 <pid>        # forced kill (SIGKILL) — last resort!
  kill -HUP <pid>      # config reload (Nginx: nginx -s reload does this)

  Docker: sends SIGTERM first, waits 10s, then SIGKILL
  Kubernetes: sends SIGTERM, waits terminationGracePeriodSeconds (30s default),
              then SIGKILL.

  In Rust (tokio):
    use tokio::signal;
    tokio::select! {
        _ = signal::ctrl_c() => {
            println!("shutting down gracefully...");
            // drain connections, flush buffers
        }
    }
```

**Choosing the right IPC:**

```
                    Need speed?   Structured   Cross-machine?   Complexity
                                  messages?
  ──────────────────────────────────────────────────────────────────────────
  Shared memory      FASTEST       No            No              High
  Unix socket        Fast          Yes (stream)  No              Low
  Pipe               Fast          No            No              Lowest
  TCP socket         Medium        Yes           YES             Low
  Message queue      Medium        Yes           No              Medium

  Rule of thumb:
    Same machine, need speed?     → Unix socket (or shared memory if extreme)
    Same machine, simple?         → Pipe
    Different machines?           → TCP socket (or gRPC/HTTP over TCP)
    Parent ↔ child, one-way?      → Pipe
    Need pub/sub or persistence?  → Message broker (Redis, Kafka — not OS IPC)
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

### Mounting — How Filesystems Become Visible

```
A disk partition is just raw bytes. Mounting ATTACHES a filesystem
(on a disk/partition/network/RAM) to a DIRECTORY in the file tree,
so you can actually read and write files on it.

  Before mount:                     After mount:
  /                                 /
  ├── home/                         ├── home/
  │   └── (empty)                   │   └── yibai/        ← files from /dev/sdb1!
  ├── mnt/                          ├── mnt/
  └── dev/                          └── dev/
      ├── sda1 (root disk)              ├── sda1
      └── sdb1 (second disk, raw)       └── sdb1

  mount /dev/sdb1 /home
  ↓
  "Take the filesystem on /dev/sdb1, make it accessible at /home"
```

**Everything is a mount.** Even your root filesystem:

```
Boot process:
  1. Kernel loads, has NO filesystem
  2. Mounts root filesystem: mount /dev/sda1 /       ← this is THE fundamental mount
  3. Reads /etc/fstab for other mounts
  4. Mounts each entry:
       /dev/sdb1   /home        ext4    defaults    0 2
       /dev/sdc1   /var/lib/pg  xfs     noatime     0 2
       tmpfs       /tmp         tmpfs   size=4G     0 0
       proc        /proc        proc    defaults    0 0
```

**The VFS (Virtual Filesystem) layer:**

```
Every file operation goes through VFS — the kernel's filesystem abstraction:

  Application: open("/home/yibai/data.txt")
       │
       ▼
  VFS: "which mount covers /home?" → /dev/sdb1 (ext4)
       │
       ▼
  ext4 driver: find inode, read blocks from /dev/sdb1
       │
       ▼
  Block layer → disk hardware

VFS is why you can mix filesystem types seamlessly:
  /        → ext4 (on SSD)
  /home    → xfs  (on HDD)
  /tmp     → tmpfs (in RAM)
  /mnt/nfs → NFS  (over network)
  /proc    → procfs (virtual, kernel-generated)

  To your code, they're all just files. open(), read(), write() — same API.
```

**Mount types you'll encounter:**

```
┌─────────────────┬──────────────────────────────────────────────────────┐
│ Type            │ What it does                                         │
├─────────────────┼──────────────────────────────────────────────────────┤
│ Block device    │ mount /dev/sda1 /mnt                                │
│                 │ Standard: attach a disk partition                     │
│                 │                                                      │
│ Bind mount      │ mount --bind /src /dst                              │
│                 │ Same filesystem, visible at TWO places               │
│                 │ Docker volumes use this: -v /host/path:/container/path│
│                 │                                                      │
│ tmpfs           │ mount -t tmpfs tmpfs /tmp -o size=2G                │
│                 │ RAM-backed filesystem. Fast, gone on reboot.         │
│                 │ Used for: /tmp, /run, Docker's container layer       │
│                 │                                                      │
│ overlayfs       │ mount -t overlay overlay -o lower=a,upper=b,work=w  │
│                 │ Layers multiple directories into one view.           │
│                 │ Docker images use this: read-only layers + writable  │
│                 │                                                      │
│ NFS             │ mount -t nfs server:/export /mnt/data               │
│                 │ Network filesystem. Remote disk appears local.       │
│                 │                                                      │
│ procfs          │ mount -t proc proc /proc                            │
│                 │ Virtual: kernel exposes process info as files.       │
│                 │ /proc/cpuinfo, /proc/[pid]/status, etc.             │
│                 │                                                      │
│ sysfs           │ mount -t sysfs sysfs /sys                           │
│                 │ Virtual: kernel exposes hardware/driver info.        │
│                 │ /sys/class/net/, /sys/block/, etc.                   │
└─────────────────┴──────────────────────────────────────────────────────┘
```

**Mount propagation (affects containers):**

```
When you mount something INSIDE a mount namespace, does it show up outside?

  Propagation type   Behavior
  ─────────────────────────────────────────────────────
  private            Mount events don't propagate at all.
                     Default for Docker containers.

  shared             Mount events propagate in BOTH directions.
                     Host mount → visible in container, and vice versa.

  slave              Host → container (one-way).
                     Container mounts stay private.

  Docker default: private. Container mounts are invisible to host.
  Kubernetes:     uses bidirectional (shared) for CSI volume plugins.
```

**Useful commands:**

```bash
mount                        # list all current mounts
findmnt                      # tree view of mount hierarchy (much cleaner)
findmnt -t ext4,xfs          # filter by filesystem type
df -h                        # disk usage per mount
lsblk                        # block devices and their mount points
cat /proc/mounts             # kernel's view of mounts (authoritative)
mount -o remount,ro /data    # remount read-only without unmounting
umount /mnt                  # detach filesystem
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

### What memory.max actually counts (kernel memory accounting)

`memory.max` is NOT just RSS. The kernel tracks all physical memory charged to the cgroup.

```
What the kernel COUNTS toward memory.max:
  ┌────────────────────────────────────────────────────────────────────┐
  │ Category          │ Counted?  │ What it is                        │
  ├───────────────────┼───────────┼───────────────────────────────────┤
  │ Anonymous (anon)  │ YES       │ Heap, stack, mmap(MAP_ANONYMOUS)  │
  │                   │           │ This is basically RSS minus file  │
  │                   │           │ pages. malloc(), Vec::new(), etc. │
  │                   │           │                                   │
  │ File-backed cache │ YES       │ Pages from read()/mmap() of files │
  │ (page cache)      │           │ Kernel caches file reads in RAM.  │
  │                   │           │ Charged to the cgroup that read   │
  │                   │           │ the file first.                   │
  │                   │           │                                   │
  │ Kernel memory     │ YES       │ Slab allocations (dentries, inodes│
  │ (kmem)            │           │ socket buffers, task_structs).    │
  │                   │           │ cgroups v2 counts this by default.│
  │                   │           │                                   │
  │ Shared memory     │ YES       │ shmem, tmpfs. Charged to the     │
  │ (shmem/tmpfs)     │           │ cgroup that created it.           │
  │                   │           │                                   │
  │ Swap              │ SEPARATE  │ Tracked by memory.swap.max,       │
  │                   │           │ NOT counted in memory.max.        │
  │                   │           │                                   │
  │ Huge pages        │ SEPARATE  │ Has its own controller.           │
  └───────────────────┴───────────┴───────────────────────────────────┘
```

```
So memory.max ≈ anon + file cache + kernel slab + shmem

This is LARGER than RSS because it includes page cache.
This is why your container can hit memory.max even if your
app's heap is small — the kernel caches file reads in RAM
and charges them to your cgroup.

Example:
  Your app uses 500MB heap (anon RSS).
  It reads 3GB of files from disk → kernel caches them (page cache).
  memory.current = ~3.5GB → hits 4GB memory.max → OOM killed!

  But wait — page cache is RECLAIMABLE. The kernel can drop it
  under pressure. So what really happens:

  1. memory.current approaches memory.max
  2. Kernel tries to RECLAIM page cache first (drop clean file pages)
  3. If enough is reclaimed → no OOM, app continues
  4. If NOT enough (too much is anon/dirty) → OOM kill
```

**RSS vs memory.current vs memory.max:**

```
  ┌──────────────────────────────────────────────────────────────────┐
  │ Metric              │ What it measures                           │
  ├─────────────────────┼────────────────────────────────────────────┤
  │ VSZ (virtual size)  │ Total virtual address space mapped.        │
  │                     │ Includes unmapped pages. MEANINGLESS for   │
  │                     │ actual memory usage. A 64-bit process can  │
  │                     │ map 128TB and use 10MB.                    │
  │                     │                                            │
  │ RSS (Resident Set)  │ Physical pages currently in RAM for this   │
  │                     │ process. = anon + file-mapped + shared.    │
  │                     │ Per-PROCESS metric (not cgroup-aware).     │
  │                     │ Double-counts shared pages! If 2 processes │
  │                     │ share a 1GB mmap, both report 1GB RSS.     │
  │                     │                                            │
  │ PSS (Proportional)  │ Like RSS but shared pages split evenly.    │
  │                     │ 1GB shared by 2 = 500MB PSS each.          │
  │                     │ Most accurate per-process metric.           │
  │                     │ Slow to compute (reads /proc/pid/smaps).   │
  │                     │                                            │
  │ memory.current      │ Actual physical memory charged to cgroup.  │
  │ (cgroup)            │ = anon + file cache + shmem + kmem.        │
  │                     │ THIS is what memory.max limits.            │
  │                     │ No double-counting — each page charged     │
  │                     │ to exactly one cgroup.                     │
  └─────────────────────┴────────────────────────────────────────────┘

  In summary:
    VSZ:             virtual, mostly useless
    RSS:             physical, per-process, double-counts shared
    PSS:             physical, per-process, fair shared accounting
    memory.current:  physical, per-cgroup, what the OOM killer uses
```

**Reading cgroup memory stats:**

```bash
# Inside a container or for a specific cgroup:
cat /sys/fs/cgroup/memory.current       # bytes currently used
cat /sys/fs/cgroup/memory.max           # the limit (memory.max)
cat /sys/fs/cgroup/memory.stat          # detailed breakdown:

  anon 524288000                         # 500MB heap/stack (non-reclaimable)
  file 3221225472                        # 3GB page cache (mostly reclaimable)
  shmem 0                                # shared memory
  kernel_stack 1048576                   # kernel stack for threads
  slab_reclaimable 20971520              # kernel slab caches (dentries, etc.)
  slab_unreclaimable 5242880             # kernel slab (non-reclaimable)

# Per-process:
cat /proc/<pid>/status | grep -E "Vm|Rss"
  VmSize:  2048000 kB    ← virtual (don't care)
  VmRSS:    512000 kB    ← physical resident (useful)

cat /proc/<pid>/smaps_rollup          # PSS (accurate but slow)
  Pss:      480000 kB
```

**Why this matters for Docker/Kubernetes:**

```
docker run --memory=4g myapp
  → sets memory.max = 4GB
  → counts EVERYTHING: your app heap + page cache + kernel buffers

Kubernetes:
  resources:
    limits:
      memory: "4Gi"       ← maps to memory.max
    requests:
      memory: "2Gi"       ← used for scheduling, not enforcement

Common surprise: "My app only uses 500MB but got OOM-killed at 4GB!"
  → Page cache from file-heavy workloads (log writing, data processing)
  → Fix: kernel reclaims page cache, but if your workload keeps
    reading new files faster than reclaim, you'll still OOM.
  → Or: set memory.high (soft limit) to trigger reclaim earlier.
```

### OOM Killer — How the Kernel Decides Who Dies

There are actually **three different OOM killers** on a modern Linux system,
and they use different metrics:

```
┌──────────────────────────────────────────────────────────────────────┐
│                    1. KERNEL OOM KILLER (traditional)                 │
│                    /proc/<pid>/oom_score                              │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│ Triggered when: the ENTIRE SYSTEM runs out of memory (no free pages  │
│ left, all reclaimable pages exhausted, swap full or disabled).       │
│                                                                      │
│ Which process to kill? → oom_score (0 to ~2000)                      │
│                                                                      │
│ The kernel computes oom_score based on:                               │
│   1. Memory usage (RSS) as % of total RAM   ← DOMINANT factor       │
│      Process using 50% of RAM → base score ~500                      │
│      Process using 0.1% → base score ~1                              │
│                                                                      │
│   2. Adjusted by oom_score_adj (-1000 to +1000)                      │
│      -1000 = NEVER kill (OOM-immune)                                 │
│         0  = default                                                 │
│      +1000 = ALWAYS kill first                                       │
│                                                                      │
│   Final: oom_score ≈ (RSS% × 1000) + oom_score_adj                  │
│   Highest score dies first.                                          │
│                                                                      │
│ It does NOT use PSS, memory.current, or cgroup stats.                │
│ It's a per-process heuristic based on RSS.                           │
│                                                                      │
│ Checking scores:                                                     │
│   cat /proc/<pid>/oom_score           # current computed score       │
│   cat /proc/<pid>/oom_score_adj       # admin-set adjustment         │
│   echo -1000 > /proc/<pid>/oom_score_adj  # make OOM-immune         │
│                                                                      │
│ Who sets oom_score_adj?                                               │
│   systemd:     OOMScoreAdjust=-900 in .service file                  │
│   Kubernetes:  Guaranteed pods → -997, Burstable → 2-999,            │
│                BestEffort → 1000 (killed first!)                     │
│   Docker:      --oom-score-adj=N                                     │
│   sshd, init:  set to -1000 by default (never kill)                  │
└──────────────────────────────────────────────────────────────────────┘
```

```
┌──────────────────────────────────────────────────────────────────────┐
│                  2. CGROUP OOM KILLER (per-container)                 │
│                  memory.max enforcement                               │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│ Triggered when: a CGROUP hits its memory.max limit                   │
│ (not system-wide — just this container/cgroup ran out)               │
│                                                                      │
│ What metric? → memory.current >= memory.max                          │
│   memory.current = anon + file cache + shmem + kmem (as above)       │
│                                                                      │
│ The sequence:                                                        │
│   1. Process in cgroup tries to allocate memory                      │
│   2. memory.current would exceed memory.max                          │
│   3. Kernel tries to reclaim: drop clean file pages, write dirty     │
│   4. If reclaim frees enough → no OOM, allocation succeeds           │
│   5. If NOT enough → invoke cgroup OOM killer                        │
│   6. Kill a process WITHIN this cgroup (not other containers!)       │
│                                                                      │
│ Which process in the cgroup? Uses oom_score within the cgroup.       │
│ Usually there's only one main process → it gets killed.              │
│                                                                      │
│ This is the most common OOM in production (Docker/K8s).              │
│ dmesg shows: "Memory cgroup out of memory: Killed process 1234"     │
│                                                                      │
│ Docker exit code: 137 (128 + 9 = SIGKILL)                           │
│   $ docker inspect <container> --format='{{.State.OOMKilled}}'      │
│   true                                                               │
│                                                                      │
│ Kubernetes: pod status = OOMKilled, gets restarted by kubelet.       │
└──────────────────────────────────────────────────────────────────────┘
```

```
┌──────────────────────────────────────────────────────────────────────┐
│              3. systemd-oomd (userspace OOM daemon)                   │
│              Proactive killing BEFORE the system is in crisis         │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│ Problem with kernel OOM: it triggers at the LAST moment when the     │
│ system is already thrashing and barely responsive. By then, the      │
│ system may be so slow it takes minutes to recover.                   │
│                                                                      │
│ systemd-oomd runs as a userspace daemon and acts EARLIER.            │
│                                                                      │
│ What metrics does it use?                                            │
│                                                                      │
│   1. memory.pressure (PSI — Pressure Stall Information):             │
│      "How much time are processes STALLED waiting for memory?"       │
│                                                                      │
│      cat /sys/fs/cgroup/some.slice/memory.pressure                   │
│        some avg10=45.00 avg60=30.00 avg300=20.00 total=123456789    │
│        full avg10=10.00 avg60=5.00  avg300=2.00  total=23456789     │
│                                                                      │
│      "some": at least one task stalled for memory (reclaim, etc.)   │
│      "full": ALL tasks stalled (nothing productive happening)        │
│                                                                      │
│      avg10 = % of the last 10 seconds spent stalled.                │
│      avg10=45 means "45% of the last 10s, tasks were memory-stalled"│
│                                                                      │
│   2. memory.current / memory.max (swap usage ratio)                  │
│      If swap usage exceeds a threshold → candidate for kill.         │
│                                                                      │
│ Decision logic:                                                      │
│   systemd-oomd monitors cgroups (systemd slices/services).           │
│   When memory.pressure exceeds threshold (default: avg10 > 60%):    │
│     → Find the cgroup using the most memory (memory.current)         │
│     → Kill it (send SIGKILL to all processes in that cgroup)         │
│     → Log it to journal: "systemd-oomd killed /my.service"          │
│                                                                      │
│ Configuration (in systemd .service or .slice):                       │
│   [Service]                                                          │
│   ManagedOOMSwap=kill            # kill if swap pressure high        │
│   ManagedOOMMemoryPressure=kill  # kill if memory pressure high      │
│   ManagedOOMMemoryPressureLimit=80%  # threshold for avg10           │
│                                                                      │
│ Enabled by default on: Fedora, Ubuntu 22.04+, RHEL 9+               │
│ Not used in Kubernetes (K8s has its own eviction manager).           │
└──────────────────────────────────────────────────────────────────────┘
```

**Summary — Three layers of OOM protection:**

```
  Trigger condition          │ Metric used           │ Scope
  ──────────────────────────┼───────────────────────┼─────────────────
  systemd-oomd               │ memory.pressure (PSI) │ per-cgroup (proactive)
  (before things get bad)    │ + memory.current      │
                             │                       │
  cgroup OOM killer          │ memory.current vs     │ per-cgroup (reactive)
  (container hits limit)     │ memory.max            │
                             │                       │
  kernel global OOM          │ oom_score (RSS-based)  │ system-wide (last resort)
  (entire system out of RAM) │ + oom_score_adj       │

  They fire in order of severity:
    systemd-oomd first → cgroup OOM second → kernel OOM last resort
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

## 7. Synchronization Primitives — Mutex, Semaphore, Condvar

When multiple threads (or processes) access shared state, you need
synchronization. These are your core tools:

### Mutex (Mutual Exclusion)

```
A mutex is a LOCK. Only ONE thread can hold it at a time.

  Thread A                Thread B
  lock(mutex)
    counter += 1          lock(mutex) ← BLOCKS (waits for A to unlock)
  unlock(mutex)
                          counter += 1
                          unlock(mutex)

Simple rule: one holder at a time. Everyone else waits.

In Rust:
  let data = Arc::new(Mutex::new(0));
  {
      let mut guard = data.lock().unwrap();  // blocks until acquired
      *guard += 1;
  }  // guard drops → lock released automatically (RAII)

In POSIX C:
  pthread_mutex_lock(&mtx);
  counter++;
  pthread_mutex_unlock(&mtx);
```

### Semaphore — The Generalized Lock

```
A semaphore is a COUNTER with atomic wait/signal operations.
It controls access to a resource with a LIMITED number of slots.

  ┌──────────────────────────────────────────────────────────────────┐
  │ Semaphore(N)                                                     │
  │                                                                  │
  │ Internal state: count (initialized to N)                         │
  │                                                                  │
  │ wait() / P() / acquire():                                        │
  │   if count > 0 → count -= 1, proceed (non-blocking)             │
  │   if count == 0 → BLOCK until someone signals                    │
  │                                                                  │
  │ signal() / V() / release():                                      │
  │   count += 1                                                     │
  │   if threads are waiting → wake one up                           │
  └──────────────────────────────────────────────────────────────────┘

  Binary semaphore (N=1): acts like a mutex
  Counting semaphore (N>1): allows up to N concurrent holders
```

### Semaphore vs Mutex — When to Use Which

```
  Mutex:                             Semaphore:
  ┌──────────────────────────────┐  ┌──────────────────────────────┐
  │ Exactly 1 holder             │  │ Up to N holders              │
  │ Owner must unlock            │  │ ANY thread can signal        │
  │ Protects a critical section  │  │ Controls concurrent access   │
  │ Has ownership (thread-bound) │  │ No ownership concept         │
  └──────────────────────────────┘  └──────────────────────────────┘

  KEY DIFFERENCE: a mutex has an OWNER — only the thread that locked it
  can unlock it. A semaphore has no owner — thread A can wait(), and
  thread B can signal(). This makes semaphores useful for SIGNALING
  between threads, not just mutual exclusion.
```

### Semaphore Use Cases

```
1. CONNECTION POOL (most common in system design)

   Semaphore(20)   ← 20 database connections available

   Thread A: sem.wait()   → count=19, gets a connection
   Thread B: sem.wait()   → count=18, gets a connection
   ...
   Thread T: sem.wait()   → count=0, BLOCKS — no connections left
   Thread A: sem.signal() → count=1, Thread T wakes up, gets connection

2. RATE LIMITING

   Semaphore(100)  ← max 100 concurrent API calls

   async fn call_api() {
       sem.acquire().await;   // blocks if 100 calls in flight
       let result = http_get(url).await;
       sem.release();
       result
   }

3. PRODUCER-CONSUMER (bounded buffer)

   Two semaphores work together:
     empty = Semaphore(BUFFER_SIZE)   ← starts full (all slots empty)
     full  = Semaphore(0)             ← starts at 0 (no items yet)

   Producer:                    Consumer:
     empty.wait()     ← wait for space   full.wait()    ← wait for item
     buffer.push(item)                    item = buffer.pop()
     full.signal()    ← signal "item ready"  empty.signal() ← signal "slot free"

   This elegantly handles:
     - Buffer full → producer blocks
     - Buffer empty → consumer blocks
     - No busy-waiting

4. THREAD-TO-THREAD SIGNALING (binary semaphore)

   Semaphore(0)  ← starts at 0

   Thread A (worker):          Thread B (coordinator):
     do_work();
     done.signal();            done.wait();  ← blocks until A signals
                                process_results();

   Can't do this with a mutex — mutex requires same thread to lock/unlock.
```

### Semaphore on Linux (kernel level)

```
Two kinds of semaphores on Linux:

POSIX semaphores (modern, preferred):
  Named:     sem_open("/my_sem", O_CREAT, 0644, N)    ← cross-process, filesystem-visible
  Unnamed:   sem_init(&sem, shared, N)                 ← thread or process-local

  sem_wait(&sem);     // decrement or block
  sem_post(&sem);     // increment, wake waiter
  sem_trywait(&sem);  // non-blocking (returns EAGAIN if would block)
  sem_timedwait();    // block with timeout

System V semaphores (legacy):
  semget(), semop(), semctl()
  More complex API, supports semaphore SETS (multiple in one object)
  Still used by PostgreSQL internally

Under the hood — futex:
  Both mutex and semaphore are built on futex (Fast Userspace muTEX):
    Fast path:  atomic compare-and-swap in userspace (no syscall!)
    Slow path:  futex(FUTEX_WAIT) → kernel blocks the thread

  Uncontended lock = ~25ns (just an atomic op, never enters kernel)
  Contended lock   = ~1-10µs (kernel involvement, thread sleep/wake)
```

### In Rust

```rust
// std::sync doesn't have a Semaphore, but tokio does:
use tokio::sync::Semaphore;

let sem = Arc::new(Semaphore::new(10));  // 10 permits

async fn limited_work(sem: Arc<Semaphore>) {
    let permit = sem.acquire().await.unwrap();  // blocks if 0 permits
    do_expensive_work().await;
    drop(permit);  // release permit (RAII, or call permit.forget() to leak)
}

// Or use acquire_owned() to move permit across tasks:
let permit = sem.clone().acquire_owned().await.unwrap();
tokio::spawn(async move {
    do_work().await;
    drop(permit);  // released when task finishes
});

// For non-async code, use std's tools:
// - Mutex for mutual exclusion
// - Condvar for signaling (condition variable)
// - Or crossbeam / parking_lot crates for fancier primitives
```

### Condition Variable (Condvar) — Semaphore's Sibling

```
A condvar lets a thread SLEEP until a condition becomes true,
re-checking the condition each time it's woken.

  Always used WITH a mutex:

  let pair = Arc::new((Mutex::new(false), Condvar::new()));

  // Waiting thread:
  let (lock, cvar) = &*pair;
  let mut ready = lock.lock().unwrap();
  while !*ready {
      ready = cvar.wait(ready).unwrap();  // releases lock, sleeps, re-acquires
  }
  // condition is true, proceed

  // Signaling thread:
  let (lock, cvar) = &*pair;
  *lock.lock().unwrap() = true;
  cvar.notify_one();  // wake one waiter (notify_all() wakes all)

Semaphore vs Condvar:
  Semaphore: "there are N resources available" (count-based)
  Condvar:   "wake up and check if your condition is true" (predicate-based)

  Condvar is more flexible but requires manual predicate checking.
  Semaphore is simpler for counting-based coordination.
```

### Common Pitfalls

```
1. DEADLOCK: Thread A holds lock X, waits for Y. Thread B holds Y, waits for X.
   Fix: always acquire locks in the same global order.

2. PRIORITY INVERSION: low-priority thread holds lock, high-priority thread
   starves waiting. Fix: priority inheritance (OS/RTOS feature).

3. FORGOTTEN SIGNAL: semaphore.wait() without matching signal → thread blocked
   forever. Use RAII guards (Rust's Drop) to ensure release.

4. SPURIOUS WAKEUP: condvar can wake without signal (POSIX allows this).
   Always put wait() in a WHILE loop, not an IF.
   while !condition { cvar.wait(); }   ← correct
   if !condition { cvar.wait(); }      ← WRONG, may proceed without condition
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
| "Semaphore vs mutex?" | Mutex = 1 holder with ownership. Semaphore = N holders, no ownership, any thread can signal. Semaphore is for limiting concurrency (connection pools, rate limiting). |
| "How does a connection pool limit connections?" | Counting semaphore initialized to pool size. acquire() before use, release() after. If all taken, caller blocks until one is returned. |
| "What's a deadlock?" | Two threads each hold a lock the other needs. Fix: global lock ordering, or try-lock with timeout. |
