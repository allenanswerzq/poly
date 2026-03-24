# CPU Architecture — What Principal Engineers Must Know

## Why This Matters

Every line of code runs on a CPU. When you're debugging latency, choosing data structures, or designing a high-performance system, CPU architecture determines what's fast and what's slow. Interviewers expect you to explain WHY something is fast, not just THAT it's fast.

## 1. CPU Core Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Modern CPU Core (simplified)                   │
│                                                                  │
│  ┌──────────────┐                                               │
│  │ Instruction   │  Fetch instructions from L1i cache            │
│  │ Fetch         │                                               │
│  └──────┬───────┘                                               │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │ Decode        │  Decode x86/ARM into micro-ops (µops)         │
│  │ (4-6 wide)   │  Modern CPUs decode 4-6 instructions/cycle    │
│  └──────┬───────┘                                               │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │ Rename +      │  Map logical registers → physical registers   │
│  │ Dispatch      │  Enables out-of-order execution               │
│  └──────┬───────┘                                               │
│         ▼                                                        │
│  ┌──────────────────────────────────────────┐                   │
│  │ Execution Units (out-of-order, parallel)  │                   │
│  │ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐│                   │
│  │ │ ALU │ │ ALU │ │ FPU │ │ Load│ │Store││                   │
│  │ │  0  │ │  1  │ │     │ │ Unit│ │ Unit││                   │
│  │ └─────┘ └─────┘ └─────┘ └─────┘ └─────┘│                   │
│  └──────────────────────────────────────────┘                   │
│         ▼                                                        │
│  ┌──────────────┐                                               │
│  │ Retire        │  Commit results in program order              │
│  │ (ROB)         │  Reorder Buffer: 200-500 entries              │
│  └──────────────┘                                               │
└─────────────────────────────────────────────────────────────────┘
```

### Key Concepts

**Superscalar**: execute multiple instructions per clock cycle (4-6 wide)

**Out-of-order execution**: CPU reorders instructions to fill execution unit gaps
```
Program order:        CPU executes:
  a = load(addr1)       a = load(addr1)     ← starts (takes 100+ cycles if cache miss)
  b = a + 1             c = 3 * 4           ← doesn't depend on 'a', run NOW
  c = 3 * 4             d = c + 5           ← doesn't depend on 'a', run NOW
  d = c + 5             b = a + 1           ← 'a' finally ready, now run this
```

**Speculative execution**: predict branch outcome, execute ahead, rollback if wrong
```
  if (x > 0) {    ← branch predictor guesses TRUE (97% accurate)
      do_work();   ← CPU starts executing this BEFORE knowing if x > 0
  }
  Prediction correct → keep results (saved ~15 cycles)
  Prediction wrong   → flush pipeline, restart (penalty: 15-20 cycles)
```

## 2. Instruction Set Architecture (ISA)

### x86-64 (Intel/AMD) — CISC
```
Complex instructions, variable length (1-15 bytes)
  ADD RAX, [RBX+RCX*4+8]    ← one instruction does: multiply, add, memory load, add
  REP MOVSB                   ← copy N bytes (loop built into one instruction)

Registers: 16 general purpose (RAX, RBX, RCX, RDX, RSI, RDI, R8-R15)
           + 16 SSE/AVX registers (128-512 bits wide for SIMD)

Decoded internally to simpler micro-ops (µops)
  1 complex x86 instruction → 1-4 µops internally
```

### ARM (AArch64) — RISC
```
Simple instructions, fixed length (4 bytes each)
  LDR X0, [X1]        ← load from memory
  ADD X0, X0, X2      ← add two registers
  STR X0, [X1]        ← store to memory
  (3 instructions to do what x86 does in 1, but each is simpler/faster to decode)

Registers: 31 general purpose (X0-X30) — more than x86!
           + 32 SIMD/FP registers (128 bits each, or SVE: variable width)

Key advantage: simpler decode → lower power → dominates mobile + now servers
  Apple M-series, AWS Graviton, Ampere Altra
```

### Why It Matters for System Design

| | x86-64 | ARM |
|---|---|---|
| Server market | Dominant (Intel Xeon, AMD EPYC) | Growing fast (Graviton, Ampere) |
| Power efficiency | Lower | Higher (better perf/watt) |
| Software compat | Everything | Most things (some need recompile) |
| Cloud cost | Standard pricing | ~20-40% cheaper (Graviton) |
| Performance | Slightly higher single-thread | Slightly higher throughput/watt |

**Interview tip**: "We'd deploy on Graviton instances for the stateless API servers — 30% cheaper at the same throughput. The database stays on x86 since PostgreSQL is better optimized there."

## 3. Cache Hierarchy — WHERE Performance Lives

```
┌───────────────────────────────────────────────────────────────┐
│  Core 0              Core 1              Core 2               │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐      │
│  │ L1i   32KB  │    │ L1i   32KB  │    │ L1i   32KB  │      │
│  │ L1d   48KB  │    │ L1d   48KB  │    │ L1d   48KB  │      │
│  │ ~1ns, 4cyc  │    │ ~1ns, 4cyc  │    │ ~1ns, 4cyc  │      │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘      │
│         │                  │                  │               │
│  ┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐      │
│  │ L2   1.25MB │    │ L2   1.25MB │    │ L2   1.25MB │      │
│  │ ~4ns, 12cyc │    │ ~4ns, 12cyc │    │ ~4ns, 12cyc │      │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘      │
│         └─────────────┬────┴──────────────┘                  │
│              ┌────────▼─────────┐                            │
│              │ L3    30-100MB   │  ← shared across all cores │
│              │ ~12ns, 40 cycles │                            │
│              └────────┬─────────┘                            │
│                       │                                      │
│              ┌────────▼─────────┐                            │
│              │ DRAM   64-2TB    │                            │
│              │ ~100ns, 300 cyc  │  ← 25x slower than L3!    │
│              └──────────────────┘                            │
└───────────────────────────────────────────────────────────────┘
```

### Cache Lines — The Unit of Data Movement
```
CPU never loads 1 byte. It loads a CACHE LINE (64 bytes).

struct Point { x: f64, y: f64 }   // 16 bytes, fits in 1 cache line
points: [Point; 1000]              // iterate sequentially → every access hits cache

vs.

struct Point { x: f64, y: f64 }
points: Vec<Box<Point>>            // pointers scattered in memory
                                    // each access → cache miss → 100ns penalty
```

### False Sharing — The Multi-Threading Performance Killer
```
Two threads on different cores, writing to different variables
that happen to be on the SAME cache line:

  Cache line: [counter_a | counter_b | padding...]
  Thread 0 (Core 0): counter_a += 1  → invalidates cache line on Core 1
  Thread 1 (Core 1): counter_b += 1  → invalidates cache line on Core 0
  → Constant cache line bouncing between cores (100x slower!)

Fix: pad variables to separate cache lines
  #[repr(align(64))]
  struct PaddedCounter {
      value: AtomicU64,
  }
```

## 4. SIMD — Single Instruction, Multiple Data

```
Scalar (1 at a time):           SIMD (4-16 at a time):
  a[0] = b[0] + c[0]             a[0..4] = b[0..4] + c[0..4]
  a[1] = b[1] + c[1]             (one instruction, 4 additions)
  a[2] = b[2] + c[2]
  a[3] = b[3] + c[3]
  (4 instructions)

x86 SIMD evolution:
  SSE:   128-bit (4 × f32)    ← 1999
  AVX2:  256-bit (8 × f32)    ← 2013
  AVX-512: 512-bit (16 × f32) ← 2017

ARM SIMD:
  NEON:  128-bit (4 × f32)
  SVE/SVE2: 128-2048 bit (scalable, hardware chooses width)

Used in: database engines (filtering), compression, JSON parsing (simdjson),
         neural network inference, image processing
```

## 5. Branch Prediction

```
Modern predictor: ~97% accurate (TAGE predictor, 4KB+ of state per branch)

When it gets it wrong (3% of branches):
  Flush pipeline → 15-20 cycle penalty

This is why:
  Sorted data is faster to process than unsorted data!

  // Unsorted: branch predictor can't learn the pattern
  for x in unsorted_data {
      if x > threshold { sum += x; }   // ~50% taken → predictor confused
  }

  // Sorted: predictor learns "first N are < threshold, rest are >"
  for x in sorted_data {
      if x > threshold { sum += x; }   // predictor: "no no no... YES YES YES"
  }
  // 2-6x faster on sorted data due to branch prediction!
```

### Branchless Programming
```rust
// Branchy (branch predictor may mispredict):
if x > 0 { y = a; } else { y = b; }

// Branchless (always same cost, no misprediction):
y = if x > 0 { a } else { b };  // compiler may use CMOV (conditional move)

// Or manually:
let mask = (x > 0) as u64;
y = a * mask + b * (1 - mask);

Used in: database engines, sorting networks, crypto (constant-time)
```

## 6. Memory Ordering & Atomics

```
CPUs reorder memory operations for performance.
Different architectures have different guarantees:

x86 (Intel/AMD): Total Store Order (TSO)
  - Store-store: ordered (stores appear in program order)
  - Load-load: ordered
  - Store-load: can be reordered! (need mfence/lock)
  - Relatively strong guarantees → easier to program

ARM: Weak ordering
  - Any operation can be reordered with any other
  - Need explicit barriers (DMB, DSB) everywhere
  - More optimization freedom → higher performance potential
  - Harder to get correct in lock-free code

Rust atomic orderings map to hardware instructions:
  Ordering::Relaxed  → no barriers (just atomic op)
  Ordering::Acquire  → load barrier (x86: free, ARM: DMB LD)
  Ordering::Release  → store barrier (x86: free, ARM: DMB ST)
  Ordering::SeqCst   → full barrier (x86: mfence, ARM: DMB ISH)
```

## 7. Modern CPU Performance Numbers

| Metric | Intel Xeon (Sapphire Rapids) | AMD EPYC (Genoa) | Apple M3 | Graviton 4 |
|--------|---------------------------|-------------------|----------|------------|
| Cores | 60 | 96 | 12 (4P+8E) | 96 |
| Frequency | 2.0-3.8 GHz | 2.3-4.1 GHz | 4.0 GHz | 2.8 GHz |
| L3 Cache | 105 MB | 384 MB | 36 MB | 96 MB |
| Memory BW | 300 GB/s | 460 GB/s | 200 GB/s | 300 GB/s |
| TDP | 350W | 360W | 22W | ~120W |

## Interview Quick Reference

| Question | Key Points |
|----------|-----------|
| "Why is array faster than linked list?" | Cache lines: sequential access → prefetch works. Pointer chasing → cache miss every node. |
| "How to optimize hot loop?" | SIMD, branchless, cache-friendly layout, avoid false sharing |
| "x86 vs ARM for servers?" | ARM: 30% better perf/watt, cheaper on cloud. x86: wider software support. |
| "Why is sorted data faster?" | Branch prediction: sorted = predictable pattern, 97%+ accuracy |
| "Explain false sharing" | Two cores write to same cache line → constant invalidation → 100x slower |
| "What is speculative execution?" | CPU guesses branch outcome, executes ahead. Wrong guess = 15 cycle penalty. Spectre exploits this. |
