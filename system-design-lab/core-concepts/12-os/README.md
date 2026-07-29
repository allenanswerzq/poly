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

### Under the Hood — How Mutex & Semaphore Are Actually Implemented

Everything bottoms out at **hardware atomic instructions** and a **kernel wait mechanism**.

#### Hardware Atomic Instructions — The Foundation

```
The CPU must guarantee that two cores don't corrupt shared state.
Regular read-modify-write (load, increment, store) is NOT safe:

  Core 0                           Core 1
  load counter → 5                 load counter → 5
  add 1 → 6                       add 1 → 6
  store counter ← 6               store counter ← 6

  Expected: 7.  Got: 6.  RACE CONDITION.

The CPU provides ATOMIC instructions that make read-modify-write indivisible:

┌──────────────────┬──────────────────────────────────────────────────────┐
│ Instruction      │ What it does                                         │
├──────────────────┼──────────────────────────────────────────────────────┤
│ CAS              │ Compare-And-Swap. Atomically:                        │
│ (cmpxchg on x86) │   if *ptr == expected { *ptr = new; return true }    │
│                  │   else { return false }                               │
│                  │ THE fundamental building block of all locks.          │
│                  │                                                       │
│ XCHG             │ Atomically swap register with memory location.       │
│ (x86)            │ Used for test-and-set spinlocks.                     │
│                  │                                                       │
│ LOCK prefix      │ x86: makes any instruction atomic.                   │
│ (x86)            │ LOCK CMPXCHG, LOCK XADD, LOCK INC                   │
│                  │ Locks the cache line across all cores.                │
│                  │                                                       │
│ LL/SC            │ Load-Linked / Store-Conditional (ARM, RISC-V, MIPS). │
│ (ARM: LDXR/STXR) │ LL: load value + mark cache line "exclusive"         │
│                  │ SC: store ONLY if no one else touched the line        │
│                  │ If SC fails → retry. No bus locking needed.          │
│                  │                                                       │
│ fetch_add        │ Atomically: old = *ptr; *ptr += val; return old      │
│ (LOCK XADD x86) │ Used for reference counting, semaphore counters.     │
│                  │                                                       │
│ Memory barriers  │ Prevent CPU/compiler from reordering loads/stores.   │
│ (fences)         │ MFENCE (x86), DMB (ARM). Ensure visibility across   │
│                  │ cores. Without barriers, Core 1 might not SEE        │
│                  │ Core 0's write for microseconds (store buffer).      │
└──────────────────┴──────────────────────────────────────────────────────┘

Cost of an atomic operation:
  Uncontended (no other core touching same cache line): ~5-20 ns
  Contended (multiple cores competing): ~50-200 ns (cache line bouncing)

Cache coherency protocol (MESI):
  When Core 0 does a CAS on address X:
    1. Core 0's cache must own the line in "Exclusive" or "Modified" state
    2. If Core 1 also has line X cached → invalidation message sent
    3. Core 1 drops its copy, Core 0 gets exclusive access
    4. CAS executes atomically on Core 0
    5. When Core 1 next reads X, it fetches the updated value

  This is why contended atomics are slow: cache lines ping-pong between cores.
```

#### Spinlock — The Simplest Lock (Pure Userspace)

```
Loop (spin) on an atomic variable until you acquire it.
No kernel involvement at all.

  Implementation (simplified):
    struct Spinlock { locked: AtomicBool }

    fn lock(&self) {
        while self.locked.compare_exchange(
            false,      // expected: unlocked
            true,       // desired: locked
            Ordering::Acquire,
            Ordering::Relaxed,
        ).is_err() {
            // CAS failed → someone else holds it → spin
            core::hint::spin_loop();  // PAUSE instruction on x86
        }
    }

    fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }

  What happens at the CPU level:
    lock():
      retry:
        LOCK CMPXCHG [locked], 1    ; atomic: if locked==0, set to 1
        JNZ retry                    ; if failed (was 1), try again
        ; MFENCE implicit in LOCK prefix — ensures ordering

    unlock():
        MOV [locked], 0             ; just store 0 (release semantics)

  PAUSE instruction (x86) / YIELD (ARM):
    Tells the CPU "I'm in a spin loop, save power, let other hyperthread run"
    Without PAUSE: spin loop saturates CPU pipeline, wastes power,
    and causes a pipeline flush when the lock is finally released (~100 cycle penalty).
    With PAUSE: ~10 cycles per iteration, much friendlier.

  When to use spinlocks:
    ✓ Lock held for very short time (< 1 µs)
    ✓ Can't afford kernel syscall overhead
    ✓ Used in: Linux kernel itself, interrupt handlers
    ✗ NEVER in userspace for long-held locks (wastes CPU)
    ✗ NEVER on single-core (spinning prevents the holder from running to unlock!)
```

#### Futex — Fast Userspace Mutex (The Real Implementation)

```
The key insight: most lock acquisitions are UNCONTENDED.
If no one else holds the lock, why involve the kernel at all?

Futex = Fast Userspace muTEX (Linux, 2003, Hubertus Franke & Rusty Russell)

  ┌──────────────────────────────────────────────────────────────────┐
  │                    FUTEX: Two Paths                               │
  │                                                                   │
  │  FAST PATH (uncontended — ~90% of the time):                     │
  │    Just an atomic CAS in userspace. NO syscall. ~25 ns.          │
  │                                                                   │
  │  SLOW PATH (contended — another thread holds the lock):          │
  │    syscall into kernel → kernel puts thread to sleep              │
  │    → thread wakes when lock is released. ~1-10 µs.               │
  │                                                                   │
  │  This is brilliant: you only pay the kernel cost when NEEDED.    │
  └──────────────────────────────────────────────────────────────────┘

How it works:

  The futex is just an int32 at a userspace memory address.
  The kernel doesn't know about it until you call futex().

  State encoding (common scheme):
    0 = unlocked
    1 = locked, no waiters
    2 = locked, there ARE waiters (kernel must be notified on unlock)

  LOCK:
    // Fast path: try to acquire (uncontended)
    if CAS(&futex, 0, 1) == success:
        return  // got the lock! No syscall. ~25 ns.

    // Slow path: someone else holds it
    loop:
        // Set state to 2 ("locked with waiters")
        // so the holder knows to wake us on unlock
        if CAS(&futex, 1, 2) or CAS(&futex, 0, 2):
            if futex == 0: return  // got it during race
        // Ask kernel to put us to sle
        // We were woken up — try to acquire again
        if CAS(&futex, 0, 2) == success:ep until futex != 2
        syscall(FUTEX_WAIT, &futex, 2)
            return  // got it!

  UNLOCK:
    old = atomic_exchange(&futex, 0)  // set to unlocked
    if old == 2:  // there were waiters
        syscall(FUTEX_WAKE, &futex, 1)  // wake one waiter

  ┌────────────────────────────────────────────────────────────────┐
  │                                                                │
  │  Thread A (holder)          Thread B (waiter)                  │
  │                                                                │
  │  CAS(0→1) ✓ acquired       CAS(0→1) ✗ failed (A holds it)    │
  │  (no syscall!)              CAS(1→2) ✓ (mark "has waiters")   │
  │                             futex(WAIT, &lock, 2) → sleeps    │
  │                                     │                         │
  │  // do work                         │ (sleeping in kernel)    │
  │                                     │                         │
  │  XCHG(lock, 0) → old=2             │                         │
  │  old was 2 → has waiters            │                         │
  │  futex(WAKE, &lock, 1)             │                         │
  │                                     │ (woken!)                │
  │                             CAS(0→2) ✓ acquired               │
  │                             (no more waiters? CAS(0→1) next)  │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘

Inside the kernel (FUTEX_WAIT):
  1. Verify *addr == expected value (avoid lost wakeup race)
  2. Hash the address → find the futex wait queue (hash table of wait queues)
  3. Add current thread to the wait queue
  4. Set thread state to TASK_INTERRUPTIBLE
  5. Schedule another thread (context switch)

  The kernel maintains a hash table of wait queues, keyed by the futex address.
  Multiple futexes may hash to the same bucket → some contention.
  This is a tradeoff for O(1) lookup vs per-futex kernel object.

Performance:
  Uncontended lock:   ~25 ns  (just CAS, no syscall)
  Contended lock:     ~1-10 µs (syscall + context switch + wakeup)
  Compare to:
    Always-syscall:   ~1 µs even uncontended (old System V semaphores)
    Spinlock:         ~5-200 ns but WASTES CPU while spinning
```

#### How pthread_mutex Is Built on Futex

```
glibc's pthread_mutex_lock() (simplified):

  int pthread_mutex_lock(pthread_mutex_t *mutex) {
      // Fast path: uncontended
      if (atomic_compare_exchange(&mutex->lock, 0, 1))
          return 0;  // acquired, no syscall!

      // Slow path: contended
      while (1) {
          // Mark as "has waiters" so unlock will wake us
          int old = atomic_exchange(&mutex->lock, 2);
          if (old == 0) return 0;  // got it during the exchange

          // Sleep until woken
          futex(&mutex->lock, FUTEX_WAIT, 2, NULL);

          // Woken up — try again
          // (might fail if another waiter grabbed it first)
      }
  }

  int pthread_mutex_unlock(pthread_mutex_t *mutex) {
      // Set to unlocked
      int old = atomic_exchange(&mutex->lock, 0);
      if (old == 2) {
          // There were waiters — wake one
          futex(&mutex->lock, FUTEX_WAKE, 1);
      }
      return 0;
  }

Rust's std::sync::Mutex on Linux:
  Same idea. Uses futex for the slow path.
  parking_lot::Mutex: even more optimized:
    - Adaptive spinning: spin briefly before sleeping (hybrid approach)
    - Smaller (1 byte vs 40 bytes for std Mutex)
    - No poisoning overhead
    - Word-sized, which means it can use LOCK CMPXCHG directly
```

#### How Semaphore Is Built on Futex

```
A counting semaphore is similar but uses an atomic counter instead of 0/1:

  struct Semaphore { count: AtomicI32 }

  fn acquire(&self) {
      loop {
          let c = self.count.load(Ordering::Relaxed);
          if c > 0 {
              // Fast path: try to decrement
              if self.count.compare_exchange(c, c - 1, ...).is_ok() {
                  return;  // got a permit, no syscall!
              }
              // CAS failed → another thread grabbed it, retry
          } else {
              // Slow path: count is 0, must wait
              futex(&self.count, FUTEX_WAIT, 0);
              // woken up → retry (count might be > 0 now)
          }
      }
  }

  fn release(&self) {
      let old = self.count.fetch_add(1, Ordering::Release);
      if old == 0 {
          // Was 0, someone might be waiting
          futex(&self.count, FUTEX_WAKE, 1);
      }
  }

Same principle as mutex:
  - Fast path (count > 0): just atomic decrement, no syscall
  - Slow path (count == 0): futex sleep, wake on release
```

#### Other Platforms (Not Just Futex)

```
┌─────────────────┬──────────────────────────────────────────────────────┐
│ Platform        │ Equivalent of futex                                  │
├─────────────────┼──────────────────────────────────────────────────────┤
│ Linux           │ futex (2003). The original.                          │
│                 │ futex2 proposed for more features.                   │
│                 │                                                      │
│ Windows         │ WaitOnAddress / WakeByAddressSingle (Win 8+).        │
│                 │ Before that: CRITICAL_SECTION (similar hybrid        │
│                 │ spin + kernel wait, but uses events internally).     │
│                 │ SRW Locks (slim reader-writer locks, very fast).    │
│                 │                                                      │
│ macOS / Darwin  │ os_unfair_lock (spin + kernel wait via ulock).      │
│                 │ psynch_mutexwait (pthread mutex kernel support).     │
│                 │ No public futex API, but kernel has similar ulock.  │
│                 │                                                      │
│ FreeBSD         │ _umtx_op (similar to futex, more general).          │
│                 │                                                      │
│ Rust            │ std::sync::Mutex → uses futex (Linux),              │
│                 │   WaitOnAddress (Windows), os_unfair_lock (macOS).  │
│                 │ parking_lot: custom implementation with adaptive    │
│                 │   spinning, uses the same OS primitives underneath. │
│                 │                                                      │
│ Go              │ runtime.lock → futex (Linux), semaphore (others).  │
│                 │ Go's goroutine parking also uses futex internally.  │
└─────────────────┴──────────────────────────────────────────────────────┘
```

#### Adaptive Spinning — The Hybrid Approach (parking_lot, Go, Java)

```
Pure futex: if lock is held, immediately sleep (syscall).
Pure spinlock: spin forever (waste CPU).

Adaptive: spin for a SHORT time, THEN sleep.

  fn lock(&self) {
      // Phase 1: try to acquire immediately
      if CAS(&lock, 0, 1) { return; }

      // Phase 2: spin briefly (~40 iterations)
      for _ in 0..SPIN_COUNT {
          if CAS(&lock, 0, 1) { return; }
          core::hint::spin_loop();  // PAUSE
      }

      // Phase 3: still not acquired → sleep (futex)
      loop {
          atomic_exchange(&lock, 2);  // mark waiters
          futex_wait(&lock, 2);
          if CAS(&lock, 0, 2) { return; }
      }
  }

Why spin first?
  If the holder is running on another core and about to release,
  spinning for ~200 ns is cheaper than a futex sleep/wake cycle (~5 µs).

  But if the holder is sleeping (on a different CPU, or timesliced out),
  spinning is pure waste → fall back to futex.

parking_lot optimizations:
  - Spin count adapts based on recent success rate
  - If spins keep succeeding → spin more
  - If spins keep failing → spin less (go to sleep faster)
  - Lock is 1 byte (vs 40 bytes for std::sync::Mutex on Linux)
```

#### The Full Stack — From Rust Mutex to Silicon

```
  your_code:   mutex.lock()
       │
       ▼
  std::sync::Mutex::lock()
       │
       ├── Fast path: CAS(0→1)     ← atomic instruction, ~25 ns
       │   (if succeeds, DONE — no syscall, no kernel)
       │
       └── Slow path: futex(FUTEX_WAIT)
               │
               ▼
           Linux kernel:
               │
               ├── Verify *addr == expected (prevent lost wakeup)
               ├── Hash futex address → find wait queue bucket
               ├── Add thread to wait queue
               ├── Set thread TASK_INTERRUPTIBLE
               └── schedule() → context switch to another thread
                       │
                       │ (thread is sleeping, uses 0 CPU)
                       │
               On unlock → futex(FUTEX_WAKE):
               ├── Find wait queue for this address
               ├── Wake one thread (or N threads)
               └── Woken thread returns from futex() syscall
                       │
                       ▼
               Woken thread retries CAS(0→1/2)
               If succeeds → lock acquired
               If fails → back to FUTEX_WAIT (another waiter got it first)

  Hardware level (what the CAS instruction actually does):
    1. CPU core issues LOCK CMPXCHG
    2. Cache coherency protocol (MESI):
       - Request exclusive ownership of cache line
       - Invalidate other cores' copies (snoop/invalidate message on bus)
       - Once exclusive: compare + swap in L1 cache (1-2 cycles)
    3. Release: write-back modified cache line
    4. Other cores see the update on their next read (cache miss → fetch)

  Full latency breakdown:
    CAS uncontended:     ~5 ns    (L1 cache hit, no bus traffic)
    CAS contended:       ~50 ns   (cache line bouncing between cores)
    Futex fast path:     ~25 ns   (CAS + function call overhead)
    Futex slow path:     ~1-10 µs (syscall + context switch)
    Context switch:      ~1-5 µs  (save/restore registers, TLB flush)
```

## 8. Memory Ordering & Memory Models

CPUs and compilers **reorder** loads and stores for performance.
On a single thread this is invisible. But with multiple threads sharing data,
reordering can cause one thread to see another thread's writes **in a different order**.

Memory ordering tells the CPU and compiler: "do NOT reorder across this boundary."

### Why Reordering Happens

```
Two independent mechanisms reorder your memory operations:

1. COMPILER reordering:
   The compiler optimizer moves loads/stores for better instruction scheduling.

   // You wrote:             // Compiler may generate:
   x = 1;                   y = 2;    ← moved up!
   y = 2;                   x = 1;

   Legal because (on one thread) the result is the same.
   But another thread might see y=2 before x=1 — violating your intent.

2. CPU reordering (hardware):
   Even if the compiler doesn't reorder, the CPU has:
   - Store buffer:  writes sit in a buffer before reaching cache/RAM.
                    Other cores can't see them yet.
   - Load buffer:   reads can complete out of order (speculative execution).
   - Write combining: adjacent writes merged into one bus transaction.

   Core 0 writes x=1 → sits in store buffer → not yet visible to Core 1.
   Core 0 writes y=2 → sits in store buffer → might flush before x=1!
   Core 1 reads y → sees 2. Reads x → still sees 0. BUG.

   ┌────────────────────────────────────────────────────────────────┐
   │  Core 0                    Core 1                              │
   │                                                                │
   │  store x = 1  ─┐                                              │
   │  store y = 2  ─┤→ store buffer                                 │
   │                 │  (not yet in cache)     load y → 2 (from cache)
   │                 │                         load x → 0 (stale!)  │
   │                 └→ eventually flush                            │
   │                    to cache                                    │
   └────────────────────────────────────────────────────────────────┘
```

### Memory Ordering Levels (from weakest to strongest)

```
┌─────────────────────────────────────────────────────────────────────────┐
│             Memory Orderings (Rust / C++ / LLVM)                        │
│                                                                          │
│  Weakest ──────────────────────────────────────────────────── Strongest  │
│                                                                          │
│  Relaxed     Acquire     Release     AcqRel        SeqCst               │
│  (no order)  (load ↓)   (store ↑)   (both)        (total order)        │
│                                                                          │
│  Cheaper ──────────────────────────────────────────────────── Costlier  │
└─────────────────────────────────────────────────────────────────────────┘
```

### Relaxed — No Ordering Guarantees

```
Ordering::Relaxed

  Guarantees: the atomic operation itself is atomic (no torn reads/writes).
  Does NOT guarantee: any ordering relative to other operations.

  Use for: counters where you don't care about ordering.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed);  // just increment, don't care about order

  Example:
    // Thread A:             // Thread B:
    x.store(1, Relaxed);    let b = y.load(Relaxed);  // might see 1
    y.store(1, Relaxed);    let a = x.load(Relaxed);  // might see 0!

    b==1, a==0 is LEGAL with Relaxed. Thread B might see y=1 before x=1
    because no ordering is enforced.

  On x86: relaxed loads/stores compile to plain MOV (no fence needed).
  On ARM: relaxed loads/stores also compile to plain LDR/STR.
  (Both are already atomic for aligned word-size accesses.)
```

### Release & Acquire — The Producer-Consumer Pair

```
Ordering::Release (for stores)
Ordering::Acquire (for loads)

  THE most important ordering pair. This is how you safely publish data.

  Release store: "everything I wrote BEFORE this store is visible to
                  anyone who does an Acquire load of this same variable."

  Acquire load:  "everything the writer wrote BEFORE their Release store
                  is now visible to me."

  ┌──────────────────────────────────────────────────────────────────┐
  │                                                                   │
  │  Thread A (producer):           Thread B (consumer):              │
  │                                                                   │
  │  DATA = 42;                     loop {                            │
  │  MORE_DATA = 100;                 if READY.load(Acquire) {        │
  │  READY.store(true, Release);        // GUARANTEED: DATA == 42     │
  │  ─── barrier ───                    // GUARANTEED: MORE_DATA == 100│
  │                                     break;                        │
  │  All writes above are             }                               │
  │  visible to B after B           }                                 │
  │  acquires READY.                                                  │
  │                                                                   │
  └──────────────────────────────────────────────────────────────────┘

  Visually, think of it as a one-way fence:

    Release (store):
      writes ↑ cannot move below this point
      ──────── Release store ────────
      (writes above are committed before this store is visible)

    Acquire (load):
      ──────── Acquire load ─────────
      reads ↓ cannot move above this point
      (reads below see everything the Release store published)

  On x86: Release store = plain MOV (x86 stores are already ordered).
          Acquire load = plain MOV (x86 loads are already ordered).
          x86 is "naturally" acquire/release — you get it for free!

  On ARM/RISC-V: Release store = STLR (store with release semantics).
                 Acquire load = LDAR (load with acquire semantics).
                 Without these, ARM reorders freely → bugs!

  This is why "it works on my x86 laptop but breaks on ARM server":
    x86 gives you acquire/release for free.
    ARM requires explicit LDAR/STLR or DMB barriers.
    Always specify the correct Ordering — don't rely on hardware being "nice".
```

### AcqRel — Both Acquire AND Release

```
Ordering::AcqRel

  For read-modify-write operations (CAS, fetch_add, swap) that both
  load AND store in one atomic operation.

  The load part has Acquire semantics.
  The store part has Release semantics.

  Example: lock acquisition with CAS
    lock.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);

    - Acquire: after we get the lock, we see all writes by the previous holder.
    - Release: when we later release, the next acquirer sees our writes.

  On x86: LOCK CMPXCHG — already has full barrier semantics.
  On ARM: generates LDAXR/STLXR (load-acquire-exclusive / store-release-exclusive).
```

### SeqCst — Sequential Consistency (Strongest)

```
Ordering::SeqCst

  All SeqCst operations appear in a SINGLE TOTAL ORDER agreed upon
  by all threads. This is the strongest guarantee and the most intuitive:
  operations happen in "program order" globally.

  ┌──────────────────────────────────────────────────────────────────┐
  │ The classic example where SeqCst matters:                        │
  │                                                                   │
  │  static X: AtomicBool = false;                                   │
  │  static Y: AtomicBool = false;                                   │
  │                                                                   │
  │  Thread A:                    Thread B:                           │
  │  X.store(true, SeqCst);      Y.store(true, SeqCst);             │
  │                                                                   │
  │  Thread C:                    Thread D:                           │
  │  if X.load(SeqCst) {         if Y.load(SeqCst) {                │
  │    assert!(Y.load(SeqCst)    assert!(X.load(SeqCst)             │
  │            || ...);                   || ...);                    │
  │  }                            }                                  │
  │                                                                   │
  │  With SeqCst: if C sees X=true, and X's store was ordered before │
  │  Y's store in the total order, then D must ALSO see X=true.     │
  │                                                                   │
  │  With Release/Acquire: no such guarantee! C and D could each     │
  │  see a different order of X and Y becoming true.                 │
  └──────────────────────────────────────────────────────────────────┘

  On x86: SeqCst store = MOV + MFENCE (or LOCK XCHG).
           The MFENCE flushes the store buffer, ensuring all previous
           stores are globally visible before any subsequent load.
           This is the ONLY ordering that costs extra on x86.

  On ARM: SeqCst store = DMB ISH + STR + DMB ISH (full barriers both sides).
           Expensive! Every SeqCst op has barriers.

  When to use SeqCst:
    ✓ When you're not sure (safest, hardest to misuse)
    ✓ When multiple atomics must be observed in the same order by all threads
    ✗ When performance matters (use Acquire/Release if possible)

  In practice: 95% of the time, Acquire/Release is sufficient and faster.
  SeqCst is for the rare case where you need a global total order.
```

### Summary Table

```
┌─────────────┬─────────────────────────────────────┬─────────────────────────┐
│ Ordering    │ What it prevents                     │ Use case                │
├─────────────┼─────────────────────────────────────┼─────────────────────────┤
│ Relaxed     │ Nothing (just atomicity)             │ Counters, statistics    │
│ Acquire     │ Reads/writes below can't move up     │ Reading a flag/lock     │
│ Release     │ Reads/writes above can't move down   │ Writing a flag/unlock   │
│ AcqRel      │ Both acquire + release               │ CAS, fetch_add in locks │
│ SeqCst      │ Total order across all threads       │ When unsure, or need    │
│             │                                      │ global ordering         │
└─────────────┴─────────────────────────────────────┴─────────────────────────┘
```

### Hardware Memory Models — Why This Varies by CPU

```
┌─────────────┬────────────────────────────────────────────────────────────┐
│ Architecture│ Memory Model                                               │
├─────────────┼────────────────────────────────────────────────────────────┤
│ x86 / x86-64│ TSO (Total Store Order) — STRONG                           │
│             │ Stores are never reordered with other stores.              │
│             │ Loads are never reordered with other loads.                │
│             │ Loads CAN be reordered before earlier stores              │
│             │   (store buffer → load can bypass pending store).          │
│             │ MFENCE prevents this (for SeqCst).                         │
│             │                                                            │
│             │ In practice: acquire/release is FREE (plain MOV).          │
│             │ Only SeqCst costs extra (needs MFENCE or LOCK).           │
│             │                                                            │
│ ARM / AArch64│ WEAK (allows almost all reorderings)                      │
│             │ Loads and stores can be reordered freely.                  │
│             │ Must use explicit barriers:                                │
│             │   LDAR (load-acquire), STLR (store-release)               │
│             │   DMB (data memory barrier)                                │
│             │                                                            │
│             │ In practice: EVERY ordering except Relaxed needs           │
│             │ special instructions. Much more expensive.                 │
│             │                                                            │
│ RISC-V     │ WEAK (like ARM). Uses FENCE instruction for barriers.      │
│             │ Also has .aq and .rl suffixes on atomics (like ARM).       │
│             │                                                            │
│ POWER (IBM) │ VERY WEAK (even weaker than ARM in some cases).            │
│             │ Allows IRIW (Independent Reads of Independent Writes)      │
│             │ — two threads can disagree on the order of two stores.     │
│             │ Needs hwsync/lwsync barriers.                             │
└─────────────┴────────────────────────────────────────────────────────────┘

The C++11 / Rust memory model is designed to work on ALL architectures:
  - You specify the INTENT (Acquire, Release, SeqCst)
  - The compiler emits the correct instructions for the target architecture
  - On x86: most orderings are free (just plain MOV)
  - On ARM: most orderings need special instructions (LDAR, STLR, DMB)
```

### Common Patterns in Practice

```
1. LAZY INITIALIZATION (Once cell / std::sync::OnceLock)

   static DATA: AtomicPtr<Config> = AtomicPtr::new(null_mut());
   static INIT: AtomicBool = AtomicBool::new(false);

   fn get_config() -> &'static Config {
       if !INIT.load(Acquire) {        // fast path: already initialized?
           // slow path: initialize
           let config = Box::leak(Box::new(load_config()));
           DATA.store(config, Release); // publish the pointer
           INIT.store(true, Release);   // publish "ready" flag
       }
       unsafe { &*DATA.load(Acquire) } // safe: acquire sees the Release
   }

   (In reality, use std::sync::OnceLock or once_cell which handle races.)


2. LOCK-FREE QUEUE (SPSC ring buffer)

   Producer:                          Consumer:
   buffer[write_idx] = item;          if read_idx != write_idx.load(Acquire) {
   write_idx.store(new_idx, Release);     item = buffer[read_idx];
   // ↑ item is written BEFORE            read_idx.store(new_idx, Release);
   //   write_idx becomes visible         // consumer sees item before idx update
                                      }

   Release on producer: ensures item is visible before index advances.
   Acquire on consumer: ensures it sees the item the producer wrote.


3. REFERENCE COUNTING (Arc)

   fetch_add(1, Relaxed) for clone:
     Just increment, no ordering needed (counter going up is always safe).

   fetch_sub(1, Release) for drop:
     Release ensures all writes to the data HAPPEN BEFORE the decrement.
     The last decrementer (who sees count reach 0) must do an Acquire fence
     to ensure they see ALL writes from all other clones before deallocation.

   // Simplified Arc::drop:
   if self.count.fetch_sub(1, Release) == 1 {
       atomic::fence(Acquire);  // see all writes before we dealloc
       drop_the_data();
   }


4. SEQLOCK (readers don't block writers — used in Linux kernel)

   Writer:                           Reader:
   seq.fetch_add(1, Release);  // odd = writing    loop {
   // write data                                       let s = seq.load(Acquire);
   seq.fetch_add(1, Release);  // even = done          if s % 2 != 0 { continue; } // writer active
                                                       let data = read_data();
                                                       if seq.load(Acquire) == s {
                                                           return data; // consistent!
                                                       }
                                                       // seq changed → writer was active, retry
                                                   }
```

### The "Happens-Before" Relationship

```
The formal model underlying all of this:

  A "happens-before" B means: B is GUARANTEED to see A's effects.

  Ways to establish happens-before:
    1. Same thread: every statement happens-before the next (program order)
    2. Release → Acquire: Release store happens-before Acquire load of same var
    3. Thread creation: spawning thread happens-before first instruction of new thread
    4. Thread join: last instruction of thread happens-before join() returns
    5. Mutex unlock → lock: unlock happens-before next lock of same mutex

  If there is NO happens-before relationship between two operations from
  different threads accessing the same non-atomic data → DATA RACE → UNDEFINED BEHAVIOR.

  Rust prevents data races at compile time (the borrow checker + Send/Sync traits).
  C++ relies on the programmer getting it right (undefined behavior if wrong).
```

### Quick Decision Guide

```
What ordering do I need?

  "Just counting something (stats, metrics)"
    → Relaxed

  "Publishing data for another thread to consume"
    → Release (writer) + Acquire (reader)

  "Implementing a lock (CAS to acquire)"
    → AcqRel for the CAS, Release for the unlock

  "I need ALL threads to agree on the order of operations"
    → SeqCst (rare, expensive on ARM)

  "I'm not sure"
    → SeqCst (correct by default, optimize later if profiling shows it matters)

  Performance difference on x86: almost none (SeqCst adds one MFENCE).
  Performance difference on ARM: significant (SeqCst adds DMB barriers everywhere).
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
