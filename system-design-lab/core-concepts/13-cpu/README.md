# CPU Architecture

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

## 8. FPU — Floating Point Unit

```
The FPU is a specialized execution unit for floating-point arithmetic.
It's separate from the ALU (integer unit) and has its own register file.

History:
  x87 FPU (legacy) → SSE scalar → AVX scalar
  ┌──────────────────────────────────────────────────────────────────┐
  │ x87 (1980s)      80-bit extended precision, stack-based (ST0-ST7)│
  │                   SLOW: out-of-order execution struggles with     │
  │                   stack model. Compiler avoids it now.            │
  │                                                                   │
  │ SSE scalar (1999) Uses XMM registers, flat register model.       │
  │                   addss xmm0, xmm1   (scalar single-precision)   │
  │                   addsd xmm0, xmm1   (scalar double-precision)   │
  │                   This is what compilers use for float/double now.│
  │                                                                   │
  │ AVX scalar (2011) Same but uses VEX encoding, avoids false deps. │
  │                   vaddss xmm0, xmm1, xmm2  (3-operand form)     │
  └──────────────────────────────────────────────────────────────────┘
```

### FPU Latency & Throughput

```
Operation          Latency (cycles)    Throughput (ops/cycle)
─────────────────────────────────────────────────────────────
ADD (f64)              3-4                   2
MUL (f64)              3-5                   2
FMA (a*b+c)            4                     2         ← fused, 1 rounding
DIV (f64)             13-22                  0.2       ← 10x slower than MUL!
SQRT (f64)            13-22                  0.2
─────────────────────────────────────────────────────────────

KEY INSIGHT: Division and sqrt are 10-50x more expensive than add/mul.
  Compilers convert "x / constant" → "x * (1/constant)" automatically.
  If you divide in a loop, hoist it out: let inv = 1.0 / divisor;

FMA (Fused Multiply-Add): a * b + c in ONE instruction
  - Only 1 rounding (vs 2 separate ops) → more accurate
  - Same latency as plain multiply → essentially free addition
  - Used EVERYWHERE: dot products, matrix multiply, polynomials
```

### IEEE 754 Gotchas That Bite in Production

```
Floating-point is NOT associative:
  (a + b) + c  ≠  a + (b + c)   in floating point!

  This means: parallelizing a sum gives DIFFERENT results.
  Reducing with SIMD (4 partial sums) ≠ scalar sequential sum.

  This is why financial systems use integer cents, not float dollars.

Denormals (subnormals) — performance trap:
  Numbers very close to zero (< 2^-1022 for f64).
  CPUs handle them in MICROCODE → 50-100x slower per operation!

  Fix: flush denormals to zero (common in audio/ML):
    _mm_setcsr(_mm_getcsr() | 0x8040);  // FTZ + DAZ flags
    // or in Rust:
    #[target_feature(enable = "sse")]
    unsafe { _mm_setcsr(_mm_getcsr() | 0x8040); }

  Warning: breaks IEEE 754 compliance. Fine for audio, bad for scientific computing.

NaN propagation:
  NaN op anything = NaN → silent corruption through entire pipeline
  Always validate inputs at system boundary, not in tight loops.
```

## 9. AVX/AVX2 — Practical SIMD Programming

```
SSE/AVX register file:
  XMM0-XMM15:   128 bits  (SSE)
  YMM0-YMM15:   256 bits  (AVX/AVX2) — lower 128 = XMM
  ZMM0-ZMM31:   512 bits  (AVX-512)  — lower 256 = YMM

┌─────────────────────────────────────────────────────────────────┐
│ ZMM0 (512 bits)                                                  │
│ ┌─────────────────────────────────────────────────────────────┐ │
│ │ YMM0 (256 bits)                                             │ │
│ │ ┌─────────────────────────────────┐                         │ │
│ │ │ XMM0 (128 bits)                 │                         │ │
│ │ │  f32  f32  f32  f32             │  f32  f32  f32  f32     │ │
│ │ └─────────────────────────────────┘                         │ │
│ └─────────────────────────────────────────────────────────────┘ │
│                                      f32  f32  f32  f32         │
│                                      f32  f32  f32  f32         │
└─────────────────────────────────────────────────────────────────┘

Data types packed into 256-bit YMM register (AVX2):
  8 × f32    or  4 × f64
  32 × i8    or  16 × i16    or  8 × i32    or  4 × i64
```

### AVX2 Intrinsics Cheat Sheet (C / Rust)

```c
// Load 8 floats from memory (must be 32-byte aligned for best perf)
__m256 a = _mm256_load_ps(ptr);       // aligned load
__m256 b = _mm256_loadu_ps(ptr);      // unaligned (tiny penalty on modern CPUs)

// Arithmetic
__m256 sum  = _mm256_add_ps(a, b);     // a + b (8 floats at once)
__m256 prod = _mm256_mul_ps(a, b);     // a * b
__m256 fma  = _mm256_fmadd_ps(a,b,c);  // a*b + c (FMA, needs FMA feature)

// Comparison → mask
__m256 mask = _mm256_cmp_ps(a, b, _CMP_GT_OQ);  // a > b ? 0xFFFFFFFF : 0

// Blend/select using mask (branchless conditional)
__m256 result = _mm256_blendv_ps(else_val, then_val, mask);

// Integer operations (AVX2, not just AVX!)
__m256i vi   = _mm256_loadu_si256((__m256i*)ptr);
__m256i vadd = _mm256_add_epi32(va, vb);          // 8 × i32 add
__m256i vcmp = _mm256_cmpeq_epi32(va, vb);        // 8 × i32 compare

// Horizontal reduction (sum all 8 floats → 1 float)
// No single instruction — need multiple shuffles:
__m128 hi  = _mm256_extractf128_ps(sum, 1);        // upper 128 bits
__m128 lo  = _mm256_castps256_ps128(sum);           // lower 128 bits
__m128 r   = _mm_add_ps(lo, hi);                    // 4 floats
r = _mm_add_ps(r, _mm_movehl_ps(r, r));             // 2 floats
r = _mm_add_ss(r, _mm_movehdup_ps(r));              // 1 float
float result = _mm_cvtss_f32(r);
```

### Rust SIMD with std::arch

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

// Runtime feature detection (safe, no unsafe needed)
if is_x86_feature_detected!("avx2") {
    unsafe { fast_path_avx2(data); }
} else {
    scalar_fallback(data);
}

#[target_feature(enable = "avx2")]
unsafe fn dot_product_avx2(a: &[f32], b: &[f32]) -> f32 {
    assert!(a.len() == b.len());
    let mut sum = _mm256_setzero_ps();

    // Process 8 floats per iteration
    for i in (0..a.len()).step_by(8) {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        sum = _mm256_fmadd_ps(va, vb, sum);  // sum += a[i..i+8] * b[i..i+8]
    }

    // Horizontal sum of 8 floats
    let hi = _mm256_extractf128_ps(sum, 1);
    let lo = _mm256_castps256_ps128(sum);
    let r = _mm_add_ps(lo, hi);
    let r = _mm_add_ps(r, _mm_movehl_ps(r, r));
    let r = _mm_add_ss(r, _mm_movehdup_ps(r));
    _mm_cvtss_f32(r)
}
```

### AVX-512 — When to Use and When to Avoid

```
AVX-512 adds:
  - 512-bit vectors (16 × f32 or 8 × f64)
  - 32 ZMM registers (vs 16 for AVX2)
  - Mask registers (k0-k7) → predicated execution per element
  - Scatter/gather, conflict detection, bit manipulation

The frequency problem:
  ┌─────────────────────────────────────────────────┐
  │ Instruction Set    Typical Freq    Relative      │
  │ ─────────────────────────────────────────────── │
  │ Scalar / SSE       3.8 GHz         1.0x          │
  │ AVX2 (256-bit)     3.5 GHz         0.92x         │
  │ AVX-512 (512-bit)  2.9 GHz         0.76x !!      │
  └─────────────────────────────────────────────────┘

  CPU THROTTLES frequency when running wide SIMD!
  If only a small fraction of your code is vectorized,
  AVX-512 can make the REST of your code slower.

  Intel "split" AVX-512 on some chips: uses 2 × 256-bit units.
  AMD Zen 4/5: AVX-512 using double-pumped 256-bit → no throttle,
               but not full 512-bit throughput.

When to use AVX-512:
  ✓ Sustained vectorized workloads (ML inference, codecs, crypto)
  ✓ Masked operations (eliminate branches in irregular data)
  ✓ Scatter/gather for sparse data access

When to avoid:
  ✗ Short bursts mixed with scalar code (frequency penalty hurts)
  ✗ Latency-sensitive code where frequency matters
  ✗ Portable code (not all x86 CPUs have it, ARM doesn't)
```

## 10. Auto-Vectorization — Let the Compiler Do It

```
Modern compilers (LLVM, GCC) automatically convert scalar loops to SIMD.
But they're conservative — many things prevent auto-vectorization.

This vectorizes:
  fn sum(data: &[f32]) -> f32 {
      data.iter().sum()   // simple reduction, no dependencies
  }
  // Compiler → _mm256_add_ps loop + scalar tail

This does NOT vectorize:
  fn running_sum(data: &[f32]) -> Vec<f32> {
      let mut acc = 0.0;
      data.iter().map(|x| { acc += x; acc }).collect()
      // Loop-carried dependency: each iteration depends on previous
  }
```

### How to Help the Compiler Vectorize

```
1. Use simple loop patterns (for i in 0..n, iterators)
2. Avoid loop-carried dependencies
3. Use &[T] slices, not linked structures
4. Avoid branches inside loops (use branchless alternatives)
5. Align data to SIMD width (32 bytes for AVX2)
6. Tell the compiler your target: RUSTFLAGS="-C target-cpu=native"

Checking if it vectorized:
  # Rust: generate assembly and look for SIMD instructions
  RUSTFLAGS="-C target-cpu=native" cargo asm my_crate::my_function

  # C/C++: compiler reports
  gcc -O3 -march=native -fopt-info-vec-missed  # what DIDN'T vectorize
  gcc -O3 -march=native -fopt-info-vec          # what DID vectorize

Look for these in the assembly:
  vaddps, vmulps, vfmadd...  → AVX/AVX2 (256-bit, "v" prefix)
  addps, mulps               → SSE (128-bit, no "v" prefix)
  vadd.f32                   → ARM NEON
```

### The portable_simd Approach (Rust Nightly)

```rust
#![feature(portable_simd)]
use std::simd::*;

// Write SIMD once, works on x86 AND ARM
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    let (a_chunks, a_tail) = a.as_simd::<8>();  // 8-wide f32 = 256 bits
    let (b_chunks, b_tail) = b.as_simd::<8>();

    let mut sum = f32x8::splat(0.0);
    for (a, b) in a_chunks.iter().zip(b_chunks) {
        sum += *a * *b;   // overloaded ops, compiles to FMA
    }

    // Handle remainder + horizontal sum
    sum.reduce_sum() + a_tail.iter().zip(b_tail).map(|(a,b)| a * b).sum::<f32>()
}
// Compiles to: AVX2 on x86, NEON on ARM. One source.
```

## 11. ARM NEON & SVE — The ARM SIMD Story

```
ARM NEON (AArch64):
  32 × 128-bit registers (V0-V31)
  Can hold: 4×f32, 2×f64, 16×i8, 8×i16, 4×i32, 2×i64

  Available on ALL AArch64 CPUs (mandatory since ARMv8)
  → no need for runtime feature detection like x86 SSE/AVX

  Intrinsics:
    float32x4_t a = vld1q_f32(ptr);          // load 4 floats
    float32x4_t c = vaddq_f32(a, b);         // 4×f32 add
    float32x4_t d = vfmaq_f32(c, a, b);      // c + a*b (FMA)
    float32x4_t m = vcgtq_f32(a, b);         // compare a > b

ARM SVE/SVE2 (Scalable Vector Extension):
  Variable-width vectors: 128 to 2048 bits (hardware decides)

  ┌────────────────────────────────────────────────────┐
  │ Write your code ONCE with SVE. The SAME binary     │
  │ runs on 128-bit Graviton and 512-bit Fugaku.       │
  │ Vector length is determined at RUNTIME.             │
  └────────────────────────────────────────────────────┘

  Key differences from x86 SIMD:
    x86 AVX: fixed width, you pick 128/256/512 at compile time
    ARM SVE: scalable, hardware picks width, one binary fits all

  Predicate registers: p0-p15 (per-element mask, like AVX-512 k-regs)
    whilelt p0.s, x0, x1     // create mask for elements < loop bound
    ld1w z0.s, p0/z, [x2]    // masked load (only active elements)
    fadd z0.s, p0/m, z0.s, z1.s  // masked add

  → Handles loop tails natively. No scalar cleanup loop needed.

SVE2 adds: crypto, integer complex operations, fixed-point.
  Used in: AWS Graviton 3+, Fujitsu A64FX (Fugaku supercomputer)
```

## 12. Detecting & Dispatching CPU Features at Runtime

```
Problem: you compile with AVX2 but user's CPU only has SSE4.
  → SIGILL (illegal instruction) crash.

Solution: runtime feature detection + dispatch.

x86 CPUID instruction tells you what's available:
  SSE, SSE2, SSE4.1, SSE4.2, AVX, AVX2, FMA, AVX-512, ...

Linux approach:
  $ cat /proc/cpuinfo | grep flags
  flags: ... sse sse2 avx avx2 fma avx512f avx512bw ...

  $ lscpu | grep -i "Model name\|Flags"
```

### Multi-Version Functions (Function Multi-Versioning)

```c
// GCC/Clang: compile same function for multiple targets
__attribute__((target("default")))
void process(float* data, int n) { /* scalar fallback */ }

__attribute__((target("avx2")))
void process(float* data, int n) { /* AVX2 version */ }

__attribute__((target("avx512f")))
void process(float* data, int n) { /* AVX-512 version */ }

// Compiler + runtime linker automatically picks the best version
// via IFUNC resolvers (GNU extension, Linux only)
```

```rust
// Rust: manual dispatch
fn process(data: &mut [f32]) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return unsafe { process_avx2(data) };
        }
    }
    process_scalar(data);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn process_avx2(data: &mut [f32]) {
    // AVX2 intrinsics here
}
```

### Compiler Target Flags

```bash
# Compile for the machine you're building on (maximum optimization)
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Compile for a specific microarchitecture
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release
#   x86-64-v1: baseline (SSE2)          — maximum compatibility
#   x86-64-v2: + SSE4.2, POPCNT         — ~2010 CPUs
#   x86-64-v3: + AVX2, FMA, BMI1/2      — ~2013 CPUs (sweet spot!)
#   x86-64-v4: + AVX-512                 — ~2017 CPUs

# GCC/Clang equivalent
gcc -O3 -march=native -mtune=native program.c
gcc -O3 -march=x86-64-v3 program.c
```

## 13. Practical Optimization Patterns

### Pattern 1: Structure of Arrays (SoA) vs Array of Structures (AoS)

```
AoS (bad for SIMD):                SoA (good for SIMD):
  struct Particle {                   struct Particles {
    x: f32,                             x: Vec<f32>,
    y: f32,                             y: Vec<f32>,
    z: f32,                             z: Vec<f32>,
    mass: f32,                          mass: Vec<f32>,
  }                                   }
  particles: Vec<Particle>

Memory layout AoS:                  Memory layout SoA:
  [x0 y0 z0 m0 | x1 y1 z1 m1 |..] [x0 x1 x2 x3 x4 x5 x6 x7 |...]
                                     [y0 y1 y2 y3 y4 y5 y6 y7 |...]

  To add all x values:               To add all x values:
  Load x0, skip y0/z0/m0,            Load x0-x7 into YMM register
  Load x1, skip y1/z1/m1,...         vaddps → 8 additions at once!
  75% of cache line wasted!           100% of cache line utilized!
```

### Pattern 2: Lookup Tables vs Compute

```
Sometimes a lookup table beats computation. Sometimes not.

When LUT wins:
  - Complex function (sin, log, exp approximation)
  - Table fits in L1 cache (< 32KB)
  - Access pattern is sequential → prefetch works

When compute wins:
  - Table is too large → cache misses
  - Function is simple → SIMD compute is faster
  - Random access pattern → cache thrashing

Modern trend: compute is getting faster, memory is getting relatively slower.
  → Lean toward computation over lookup tables.

Example: popcount (count set bits)
  Old way: 256-byte lookup table
  Modern: popcnt instruction (1 cycle!) or SIMD vpshufb-based
```

### Pattern 3: Prefetching

```
CPU hardware prefetcher detects sequential and strided access patterns.
For irregular patterns, use software prefetch hints:

// C
__builtin_prefetch(ptr + 64, 0, 3);   // prefetch for read, all cache levels

// Rust (nightly or via intrinsics crate)
// Generally: let the hardware prefetcher do its job.
// Software prefetch helps for:
//   - Pointer-chasing (linked lists, trees, hash tables)
//   - Known-ahead irregular access patterns

// Example: hash table probe
for key in keys {
    let idx = hash(key) % table.len();
    prefetch(&table[idx + BATCH_SIZE]);  // prefetch ahead
    process(&table[idx]);
}
```

## Interview Quick Reference

| Question | Key Points |
|----------|-----------|
| "Why is array faster than linked list?" | Cache lines: sequential access → prefetch works. Pointer chasing → cache miss every node. |
| "How to optimize hot loop?" | SIMD, branchless, cache-friendly layout, avoid false sharing |
| "x86 vs ARM for servers?" | ARM: 30% better perf/watt, cheaper on cloud. x86: wider software support. |
| "Why is sorted data faster?" | Branch prediction: sorted = predictable pattern, 97%+ accuracy |
| "Explain false sharing" | Two cores write to same cache line → constant invalidation → 100x slower |
| "What is speculative execution?" | CPU guesses branch outcome, executes ahead. Wrong guess = 15 cycle penalty. Spectre exploits this. |
| "When to use SIMD/AVX?" | Sustained data-parallel workloads: filtering, compression, ML inference. Avoid for scalar-heavy code with occasional vectorizable bits (freq throttling). |
| "FPU vs ALU?" | FPU handles float/double; add/mul ~4 cycles, div/sqrt ~20 cycles. Division is expensive — multiply by reciprocal. FMA gives multiply+add in 1 instruction. |
| "AoS vs SoA?" | SoA for SIMD: group same fields together → load 8 values into one register. AoS wastes 75% of cache line when accessing one field. |
| "What's the deal with denormals?" | Tiny floats near zero. CPU handles in microcode → 50-100x slower. Flush to zero in audio/ML workloads. |
| "How does AVX-512 differ from AVX2?" | 512-bit regs + mask registers + scatter/gather. But CAUSES CPU FREQUENCY THROTTLE. Only use for sustained vectorized workloads. |
| "How does ARM SVE differ from x86 SIMD?" | SVE is scalable: vector width determined at runtime by hardware. One binary works across different ARM chips. x86 SIMD is fixed-width, chosen at compile time. |
