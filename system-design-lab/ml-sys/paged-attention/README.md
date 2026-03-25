# PagedAttention — How vLLM Manages KV Cache Like an OS

## What It Is

PagedAttention is vLLM's core innovation. It manages GPU memory for KV caches the same way an operating system manages virtual memory — with pages, page tables, and on-demand allocation.

## The Problem

```
LLM inference: each active request needs a KV cache.
KV cache per token = num_layers × num_kv_heads × head_dim × 2 (K+V) × 2 bytes (fp16)

LLaMA-70B per token: 80 layers × 8 kv_heads × 128 dim × 2 × 2 = 327 KB
At max_seq_len=4096: 327KB × 4096 = 1.3 GB per request

THE PROBLEM: you must pre-allocate for max_seq_len.
  Request A generates 100 tokens: allocates 1.3 GB, uses 32 MB (2.5%)
  Request B generates 4000 tokens: allocates 1.3 GB, uses 1.27 GB (98%)
  Average: ~50% of GPU memory is wasted on empty pre-allocated space.

  With 80GB GPU, 60GB for KV cache:
    Pre-allocated: 60GB / 1.3GB = 46 concurrent requests max
    Average utilization: ~23 requests worth of actual data
    WASTE: 50% of your expensive GPU memory sits empty.
```

## PagedAttention — The Solution

```
Instead of contiguous pre-allocated blocks per request,
allocate KV cache in small PAGES (like OS virtual memory).

Page size: 16 tokens of KV cache (typical)
  1 page = 16 × 327KB/token = ~5.2 MB (for LLaMA-70B)

Physical pages: a pool of fixed-size memory blocks in GPU HBM
Page table: per-request mapping of logical → physical pages

Request A (100 tokens): needs ceil(100/16) = 7 pages = 36 MB
Request B (4000 tokens): needs ceil(4000/16) = 250 pages = 1.3 GB
Request C (10 tokens): needs ceil(10/16) = 1 page = 5.2 MB

No pre-allocation! Pages allocated on demand as tokens are generated.
Waste: at most 1 page per request (last page partially filled) → <4% waste

  With 80GB GPU, 60GB for KV cache:
    Page pool: 60GB / 5.2MB = ~11,500 pages
    Can serve: many more concurrent requests → 2-4x throughput improvement
```

## Page Table — How It Works

```
Physical memory (GPU HBM):
  ┌──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┐
  │Page 0│Page 1│Page 2│Page 3│Page 4│Page 5│Page 6│ ...  │
  └──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┘

Request A's page table:    [Page 3, Page 0, Page 5, Page 1, ...]
  Logical token 0-15   → Physical Page 3
  Logical token 16-31  → Physical Page 0
  Logical token 32-47  → Physical Page 5
  (pages are NOT contiguous in memory — just like virtual memory!)

Request B's page table:    [Page 2, Page 4, Page 6, ...]
  Logical token 0-15   → Physical Page 2
  Logical token 16-31  → Physical Page 4

Free page pool: [Page 7, Page 8, Page 9, ...]

When Request A generates token 48 (needs a new page):
  Pop Page 7 from free pool → append to A's page table
  Write new KV into Page 7

When Request A finishes:
  Return all of A's pages to free pool: [Page 3, Page 0, Page 5, Page 1, ...]
```

## Copy-on-Write — Enabling Beam Search

```
Beam search: multiple beams share the same prefix tokens.
Without paging: must copy the entire KV cache for each beam.
With paging: beams share prefix PAGES, only copy when they diverge.

  Beam 1 page table: [Page 3, Page 0, Page 5, Page 1, Page 7]
  Beam 2 page table: [Page 3, Page 0, Page 5, Page 1, Page 8]
                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
                       shared! (ref count = 2)         ^^^^
                                                       diverged

  Page 3, 0, 5, 1: ref_count = 2 (both beams point here)
  Page 7: ref_count = 1 (Beam 1 only)
  Page 8: ref_count = 1 (Beam 2 only)

  If Beam 2 needs to MODIFY a shared page:
    Copy the page first (copy-on-write), then modify the copy.

  For beam search with 4 beams and 1000 prefix tokens:
    Without CoW: 4 × 1000 tokens of KV cache copied = 4x memory
    With CoW:    1000 tokens shared + only divergent tokens copied = ~1.1x memory
```

## Prefix Caching (Extension)

```
Many requests share the same system prompt:
  "You are a helpful assistant. Be concise and accurate."

Without prefix caching:
  Request 1: compute KV for system prompt (100 tokens) + user query
  Request 2: compute KV for system prompt (100 tokens) + user query
  Request 3: compute KV for system prompt (100 tokens) + user query
  → Same computation repeated 1000x per second!

With prefix caching:
  Compute system prompt KV cache ONCE → store in shared pages
  All requests reference the same physical pages for the prefix
  Only compute KV for the unique user query part

  Speedup: for a 500-token system prompt and 100-token query,
  prefix caching skips 83% of the prefill computation.

SGLang's RadixAttention takes this further:
  Radix tree of all cached prefixes (not just system prompts)
  Multi-turn conversations reuse previous turn's KV cache automatically
```

## Comparison with OS Virtual Memory

```
OS Virtual Memory          PagedAttention
─────────────────          ──────────────
Process                    Request
Virtual address space      Logical KV cache (token positions)
Physical page frames       Physical KV cache pages (GPU HBM)
Page table                 Block table (per request)
Page fault → allocate      New token → allocate page
Process exit → free pages  Request done → free pages
Fork → copy-on-write       Beam search → copy-on-write
Shared libraries → shared  System prompt → prefix cache
Swap to disk               Offload to CPU (future work)
```
