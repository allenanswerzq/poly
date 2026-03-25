# Flash Attention — How IO-Aware Attention Works

## What It Is

Flash Attention rewrites the attention computation to minimize GPU memory reads/writes (HBM access). Same math, same output, but 2-4x faster and uses O(N) memory instead of O(N²).

## The Problem

```
Standard attention:
  S = Q @ K^T        → (N, N) matrix, stored in HBM         ← O(N²) memory
  P = softmax(S)     → (N, N) matrix, stored in HBM         ← O(N²) memory
  O = P @ V          → (N, d) matrix                        ← output

  For N=8192, d=128:
    S matrix: 8192² × 4 bytes = 256 MB
    P matrix: 8192² × 4 bytes = 256 MB
    Total intermediate: 512 MB per head, per layer!

  The bottleneck is NOT compute — it's reading/writing these huge matrices
  from GPU HBM (global memory). GPUs have enormous compute (312+ TFLOPS)
  but limited memory bandwidth (2-3 TB/s).

Hardware reality:
  A100 compute: 312 TFLOPS (fp16)
  A100 HBM bandwidth: 2 TB/s
  A100 SRAM (on-chip): 20 MB, ~19 TB/s bandwidth

  Arithmetic intensity = FLOPs / bytes
  Standard attention: ~O(1) — compute one element, read/write it
  → MEMORY BOUND. GPU compute sits mostly idle!
```

## The Flash Attention Insight

```
Instead of materializing the N×N matrix in HBM,
compute attention in TILES that fit in SRAM (on-chip memory).

Standard:
  HBM ──read Q,K──► compute S ──write S to HBM──► read S ──► softmax ──write P to HBM──► read P,V ──► output
  6 HBM reads/writes for intermediate results!

Flash Attention:
  HBM ──read Q block, K block, V block──► compute everything in SRAM ──► write output to HBM
  Only 2 HBM accesses (read input, write output). No intermediate storage!
```

## The Algorithm

```
Inputs: Q, K, V in HBM (each N×d)
Output: O in HBM (N×d)

Divide Q into blocks of size Br, K/V into blocks of size Bc.
Br, Bc chosen so that blocks fit in SRAM (~20 MB on A100).

For each Q block (i = 0, 1, ..., N/Br):
  Load Q_i from HBM to SRAM                    ← 1 read
  Initialize: O_i = 0, m_i = -inf, l_i = 0    (running max, running sum)

  For each K,V block (j = 0, 1, ..., N/Bc):
    Load K_j, V_j from HBM to SRAM             ← 1 read per block

    Compute: S_ij = Q_i @ K_j^T / sqrt(d)      ← in SRAM, fast!

    Online softmax update:
      m_new = max(m_i, max(S_ij))              ← new running max
      P_ij = exp(S_ij - m_new)                 ← local softmax (not global!)
      l_new = exp(m_i - m_new) * l_i + sum(P_ij)  ← rescaled running sum

      O_i = (exp(m_i - m_new) * l_i * O_i + P_ij @ V_j) / l_new
      ↑ the key: rescale previous accumulation when max changes

      m_i = m_new
      l_i = l_new

  Write O_i to HBM                              ← 1 write

Memory: O(N) — only store O, m, l (1 value per row), never the N×N matrix
Compute: same FLOPs as standard attention (same matmuls)
IO: dramatically fewer HBM accesses → 2-4x faster
```

## Online Softmax — The Key Trick

```
Problem: softmax needs ALL scores to compute the denominator.
  softmax(x_i) = exp(x_i - max) / Σ exp(x_j - max)
  You need the global max and global sum before you can produce any output.

Online softmax: maintain running max (m) and running sum (l).
When you see a new block of scores:

  If new max > old max:
    l = exp(old_max - new_max) × l + Σ exp(new_scores - new_max)
    ↑ rescale previous sum to account for the larger max

    O = exp(old_max - new_max) × O + P_new @ V_new
    ↑ also rescale the running output

Result: mathematically EXACT (not approximate). Bit-for-bit identical output.
```

## Flash Attention 2 and 3

```
Flash Attention 2 (2023):
  - Better work partitioning: parallelize over sequence length, not batch
  - Reduce non-matmul FLOPs (softmax bookkeeping)
  - ~2x faster than Flash Attention 1

Flash Attention 3 (2024, H100-specific):
  - Exploits H100 asynchronous execution (TMA + WGMMA)
  - Warp specialization: separate warps for different tasks
  - Overlaps computation with memory loads
  - FP8 support (2x over FP16)
  - ~1.5x faster than Flash Attention 2 on H100
```

## When Flash Attention Helps Most

```
                      Standard    Flash Attn    Speedup
seq_len=512           baseline    ~1.3x         small (overhead dominant)
seq_len=2048          baseline    ~2x           meaningful
seq_len=8192          baseline    ~3x           significant
seq_len=32768         OOM!        works fine    ∞ (enables long context)

Flash Attention is most impactful for LONG sequences because:
  Standard: O(N²) memory → OOM at ~8K on A100
  Flash:    O(N) memory  → can do 128K+ on the same GPU
```
