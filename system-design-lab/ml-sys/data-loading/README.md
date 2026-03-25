# Data Loading — How Training Data Gets to the GPU

## Why It Matters

The GPU can process a batch in ~100ms. If data loading takes 200ms, the GPU sits idle 50% of the time. You're paying $3/hour for a GPU that's working at half capacity. Data loading must be FASTER than training to keep the GPU fed.

## The Pipeline

```
Disk (TB of tokenized data)
    │
    │ memory-mapped file read (mmap)
    ▼
CPU RAM (DataLoader workers, num_workers=8+)
    │
    │ collate: pad/pack sequences into batch
    │ pin memory (pin_memory=True)
    ▼
Pinned CPU Memory (DMA-accessible)
    │
    │ async copy: cuda.stream → PCIe/NVLink transfer
    ▼
GPU HBM (ready for training)

Key: overlap each stage with the next.
While GPU trains on batch N:
  CPU prepares batch N+2
  PCIe transfers batch N+1
→ GPU never waits for data.
```

## Pre-Tokenization

```
BAD: tokenize at train time
  Raw text → tokenizer → token IDs → batch → GPU
  Tokenization is 10-100x slower than GPU training!

GOOD: pre-tokenize and save as binary
  Offline: raw text → tokenizer → save as numpy/mmap binary file
  Training: load binary tokens directly → batch → GPU

  Format: flat array of token IDs (uint16 or uint32)
  File structure:
    shard_0000.bin: [token_0, token_1, ..., token_1M]
    shard_0001.bin: [token_1M+1, ..., token_2M]
    ...
    shard_0999.bin: last shard

  Each shard: ~1-10GB. Total dataset: 1-100TB.
  Shards shuffled across epochs. Workers read different shards.
```

## Sequence Packing

```
Problem: sequences have different lengths → padding wastes compute.

Without packing (padding):
  [Hello world <pad> <pad> <pad> <pad> <pad> <pad>]  seq_len=8, useful=2  (25%)
  [The quick brown fox <pad> <pad> <pad> <pad>]       seq_len=8, useful=4  (50%)
  [A very long sentence with many words in it]         seq_len=8, useful=8 (100%)
  Average utilization: 58%. You pay for 100% of compute but use 58%.

With packing:
  [Hello world <sep> The quick brown fox <sep> ...]    seq_len=8, useful=8 (100%)
  [A very long sentence with many words in it]         seq_len=8, useful=8 (100%)
  Average utilization: 100%.

  Attention mask prevents cross-contamination:
    Token 0 (Hello) can attend to token 1 (world) but NOT token 3 (The)
    because they're from different documents.

  Typical speedup: 1.5-3x for datasets with variable-length sequences.
```

## Memory-Mapped Loading (mmap)

```
Normal file reading:
  Read file → copy to kernel buffer → copy to user buffer → use it
  Problem: for 100GB datasets, you need 100GB of RAM just to load it.

Memory mapping (mmap):
  Map file directly into virtual address space.
  OS pages in/out on demand. Only data you access gets loaded.

  dataset = np.memmap("shard_0.bin", dtype=np.uint16, mode="r")
  batch = dataset[offset : offset + batch_size * seq_len]
  # ↑ only reads the pages containing this range, NOT the whole file

  Benefits:
    - No extra RAM needed (OS manages page cache)
    - Random access without loading entire file
    - Multiple workers can share the same mapping
    - OS prefetches ahead automatically
```

## Distributed Data Loading

```
8 GPUs, 1000 shards, each GPU must see different data:

  GPU 0: shards [0, 8, 16, 24, ...]
  GPU 1: shards [1, 9, 17, 25, ...]
  GPU 2: shards [2, 10, 18, 26, ...]
  ...

  DistributedSampler assigns disjoint subsets.
  Shuffling: within each GPU's shards + across epochs.

  Guarantee: no two GPUs see the same data in the same epoch.
  Over multiple epochs: every GPU eventually sees all data.
```

## Key Performance Numbers

```
Source            Bandwidth     Time for 1GB batch
───────────────────────────────────────────────────
NVMe SSD          7 GB/s        ~140ms
RAID SSD array    20+ GB/s      ~50ms
CPU RAM (mmap)    50+ GB/s      ~20ms
Pinned → GPU      32 GB/s       ~30ms (PCIe 5.0)
GPU HBM           2 TB/s        ~0.5ms

Typical batch: ~10MB (batch=32, seq=2048, 2 bytes/token)
From SSD to GPU: ~1.5ms + ~0.3ms = ~2ms total
Training step: ~100ms
→ Data loading is ~2% of step time. GPU stays busy.

If data loading becomes the bottleneck:
  1. Increase num_workers (8-16 CPU processes)
  2. Use faster storage (NVMe RAID, not spinning disks)
  3. Pre-fetch more batches (prefetch_factor=4)
  4. Pre-tokenize to binary format (skip tokenization)
  5. Use webdataset/mosaic streaming for cloud storage
```

