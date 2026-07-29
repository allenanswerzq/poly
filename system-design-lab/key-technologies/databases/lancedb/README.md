# LanceDB Deep Dive

## Overview

LanceDB is an **embedded vector database** built on the Lance columnar format. It's designed for AI/ML workloads — storing embeddings alongside their metadata and doing fast approximate nearest neighbor (ANN) search. Think "DuckDB for vectors" — no server, runs in-process, but optimized for high-dimensional vector similarity search.

## History & Why It Exists

```
The problem (2022):
  ML engineers building AI applications needed to:
  1. Store embeddings (1536-dim float vectors from OpenAI, etc.)
  2. Search by similarity (nearest neighbors)
  3. Filter by metadata (WHERE category = 'tech' AND date > '2024-01')
  4. Handle multi-modal data (text, images, audio alongside vectors)

  Their options were:
  - Pinecone/Weaviate/Milvus → separate server, vendor lock-in, expensive
  - pgvector → bolt-on to PostgreSQL, not optimized for vector workloads
  - FAISS → in-memory only, no persistence, no metadata filtering
  - ChromaDB → Python-only, SQLite under the hood, doesn't scale

  Chang She and Lei Xu (ex-Databricks engineers) thought:
  "What if we built a new columnar format specifically designed for
  ML data (vectors + multi-modal), and an embedded database on top?"

  They created Lance (the format) and LanceDB (the database).

Timeline:
  2022  LanceDB founded (Chang She, Lei Xu)
  2022  Lance format v1 — columnar format with native vector support
  2023  LanceDB open-source release, IVF-PQ index
  2023  DiskANN index support (Microsoft's graph-based ANN)
  2024  Lance v2 format (better encoding, faster scans)
  2024  LanceDB Cloud (managed service)
  2025  Production adoption for RAG pipelines, multi-modal search

Key design philosophy:
  - Embedded: no server, import as a library (Python, JS, Rust)
  - Vector-native: first-class support for high-dimensional vectors
  - Multi-modal: store images, text, embeddings, metadata together
  - Columnar: Lance format — like Parquet but optimized for vectors + random access
  - Zero-copy: memory-mapped, versioned (like Git for data)
  - Disk-based ANN: search billions of vectors without loading all into RAM
```

## Why Not Just Use Parquet + FAISS?

```
┌──────────────────────┬──────────────────┬──────────────────┬──────────────────┐
│                      │ FAISS            │ Parquet+pgvector │ LanceDB          │
├──────────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Vector search        │ ✓ Fast (in-mem)  │ ✓ Slow (B-tree)  │ ✓ Fast (disk)    │
│ Metadata filtering   │ ✗ No             │ ✓ Yes             │ ✓ Yes            │
│ Persistence          │ ✗ Manual save    │ ✓ Yes             │ ✓ Yes (versioned)│
│ Larger-than-RAM      │ ✗ Must fit in RAM│ ✓ Yes             │ ✓ Yes            │
│ Update/delete        │ ✗ Rebuild index  │ ✓ Yes             │ ✓ Yes (MVCC)     │
│ Multi-modal data     │ ✗ Vectors only   │ ✗ Awkward         │ ✓ Native         │
│ Server needed        │ No               │ Yes (PostgreSQL)  │ No               │
│ Random access        │ N/A              │ ✗ Scan only       │ ✓ O(1) by rowid  │
└──────────────────────┴──────────────────┴──────────────────┴──────────────────┘
```

## Architecture

### The Full Stack

```
┌──────────────────────────────────────────────────────────────────────────┐
│           LanceDB Architecture                                           │
│                                                                          │
│  Query: Find 10 nearest vectors to query_vec WHERE category = 'tech'    │
│                                                                          │
│  ┌──────────────────┐                                                   │
│  │  Query API        │  Python/JS/Rust SDK                              │
│  │                   │  table.search(query_vec).where("cat='tech'")     │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Query Planner    │  Decides: use ANN index? Pre-filter or           │
│  │                   │  post-filter metadata? Which partitions?         │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Vector Index     │  IVF-PQ: partition + compress vectors            │
│  │                   │  DiskANN: graph-based, disk-resident             │
│  │                   │  Flat: brute-force (small datasets)              │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│  ┌──────────────────┐                                                   │
│  │  Lance Format     │  Columnar storage with random access             │
│  │  (Storage Layer)  │  Versioned (append-only, like Git)               │
│  │                   │  Memory-mapped I/O                               │
│  └──────┬───────────┘                                                   │
│         ▼                                                                │
│      data.lance/  (directory on local disk, S3, or GCS)                 │
└──────────────────────────────────────────────────────────────────────────┘
```

### 1. Lance Format — Why Not Parquet?

```
Parquet was designed for batch analytics (scan entire columns).
Lance was designed for ML workloads (random access + vectors).

Key differences:

┌────────────────────────┬──────────────────────┬──────────────────────┐
│                        │ Parquet               │ Lance                │
├────────────────────────┼──────────────────────┼──────────────────────┤
│ Optimized for          │ Full column scans     │ Random access + scan │
│ Read row by ID         │ ✗ Must scan           │ ✓ O(1) lookup        │
│ Append data            │ Write new file        │ Append fragment      │
│ Update/delete          │ Rewrite entire file   │ Append new version   │
│ Vector columns         │ Stored as binary blob │ Native, searchable   │
│ Versioning             │ No                    │ Yes (Git-like)       │
│ Encoding               │ RLE, dict, delta      │ Same + vector-aware  │
└────────────────────────┴──────────────────────┴──────────────────────┘

Lance file structure:

  ┌──────────────────────────────────────────────────┐
  │  Lance File                                       │
  │                                                   │
  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  │
  │  │ Data pages │  │ Data pages │  │ Data pages │  │
  │  │ (column 1) │  │ (column 2) │  │ (vectors)  │  │
  │  │ "text"     │  │ "category" │  │ [0.1, 0.3  │  │
  │  │            │  │            │  │  0.7, ...]  │  │
  │  └────────────┘  └────────────┘  └────────────┘  │
  │                                                   │
  │  ┌─────────────────────────────────────────────┐  │
  │  │ Column metadata + page index                 │  │
  │  │ (enables O(1) random access by row ID)       │  │
  │  └─────────────────────────────────────────────┘  │
  │                                                   │
  │  ┌─────────────────────────────────────────────┐  │
  │  │ Manifest (schema, version, fragment list)    │  │
  │  └─────────────────────────────────────────────┘  │
  └──────────────────────────────────────────────────┘

Random access trick:
  Parquet: to read row 1,000,000, scan from the start of the row group.
  Lance: page index maps row_id → exact byte offset. Seek directly.
  This is critical for ANN search — you need to fetch specific vectors
  by ID after the index narrows down candidates.

Versioning (MVCC):
  Each write creates a new version (new manifest + new data fragments).
  Old data is never overwritten — zero-copy time travel.

  v1: [fragment-0, fragment-1]
  v2: [fragment-0, fragment-1, fragment-2]        ← append
  v3: [fragment-0, fragment-1', fragment-2]       ← update (new fragment-1')

  Old versions are kept until compacted (like Git garbage collection).
```

### 2. Vector Index — How ANN Search Works

```
Brute-force (flat) search:
  Compare query vector to EVERY stored vector.
  O(N × D) where N = num vectors, D = dimensions.
  100M vectors × 1536 dims = way too slow.

ANN (Approximate Nearest Neighbor) indexes trade accuracy for speed.
LanceDB supports two main index types:

═══════════════════════════════════════════════════════════════════
IVF-PQ (Inverted File Index + Product Quantization)
═══════════════════════════════════════════════════════════════════

Step 1: IVF — Partition vectors into clusters (like K-means)
  Train K centroids (e.g., K=256) on a sample of vectors.
  Assign each vector to its nearest centroid.

  ┌─────────┐  ┌─────────┐  ┌─────────┐
  │Cluster 0│  │Cluster 1│  │Cluster 2│  ... K clusters
  │ vec 3   │  │ vec 0   │  │ vec 2   │
  │ vec 7   │  │ vec 5   │  │ vec 4   │
  │ vec 9   │  │ vec 8   │  │ vec 6   │
  └─────────┘  └─────────┘  └─────────┘

  At query time: find nearest nprobe centroids (e.g., 10),
  only search vectors in those clusters. Skip the rest.
  10/256 = ~4% of data searched.

Step 2: PQ — Compress each vector to save memory + disk

  Original: 1536-dim float32 = 6144 bytes per vector
  PQ splits into 96 subvectors of 16 dims each.
  Each subvector quantized to 1-byte codebook entry.
  Compressed: 96 bytes per vector (64x smaller!)

  ┌───────────────────────────────────────────┐
  │ Original vector (1536 floats = 6144 bytes)│
  │ [0.12, 0.45, ..., 0.78, 0.33, ...]       │
  └───────────────────────────────────────────┘
                    ▼ PQ compress
  ┌───────────────────────────────────────────┐
  │ Compressed (96 bytes)                      │
  │ [42, 187, 3, 255, 91, ...]                │
  │  ▲ each byte = index into learned codebook│
  └───────────────────────────────────────────┘

  Distance computed using precomputed lookup tables — very fast.

═══════════════════════════════════════════════════════════════════
DiskANN (Microsoft, 2019) — Graph-based, disk-resident
═══════════════════════════════════════════════════════════════════

Build a navigable graph where vectors are nodes and edges connect
similar vectors. Search by traversing the graph from an entry point.

  ┌───┐     ┌───┐     ┌───┐
  │ A │────►│ B │────►│ C │ ← nearest to query
  └─┬─┘     └─┬─┘     └───┘
    │         │
    ▼         ▼
  ┌───┐     ┌───┐
  │ D │     │ E │
  └───┘     └───┘

  Why DiskANN is special:
  - Graph stored on disk (SSD), not RAM
  - Small "compressed" version in RAM for navigation
  - Full vectors fetched from disk only for final candidates
  - Can handle billions of vectors with limited RAM

  Search:
  1. Start at entry node
  2. Greedily move to neighbor closest to query
  3. Fetch full vectors from disk for top candidates
  4. Return K nearest

  Memory: ~10-30 bytes per vector in RAM (compressed PQ vectors)
  Disk: full vectors + graph edges on SSD
  1 billion vectors × 30 bytes = ~30 GB RAM (vs 6 TB for full vectors)
```

### 3. Filtered Search — Pre-filter vs Post-filter

```
Query: "Find 10 nearest vectors WHERE category = 'tech'"

Post-filter (naive):
  1. Find 10 nearest vectors (ignoring filter)
  2. Remove non-matching ones
  Problem: might return fewer than 10 results!
  Workaround: overfetch (find 100, filter, take 10) — wasteful.

Pre-filter:
  1. Find all rows where category = 'tech' (bitmap)
  2. Search only those vectors
  Problem: if filter is very selective, may search tiny subset —
  the ANN index (built on all data) becomes useless.

LanceDB's approach — adaptive:
  - Selective filter (few matches)? → pre-filter + brute-force on subset
  - Broad filter (most match)?     → use ANN index + post-filter
  - Middle ground?                 → pre-filter + search filtered partitions

  The query planner decides based on estimated selectivity.
```

## Why It's Fast — Summary

```
┌────────────────────────────────────────────────────────────────────────┐
│  Technique              │ What it does              │ Impact           │
├─────────────────────────┼───────────────────────────┼──────────────────┤
│ IVF partitioning        │ Search ~5% of data        │ 20x less compute │
│ Product Quantization    │ 64x vector compression    │ Fits in RAM/cache│
│ DiskANN graph           │ ~100 SSD reads per query  │ Billions on disk │
│ Lance random access     │ O(1) vector fetch by ID   │ No full scans    │
│ Memory-mapped I/O       │ OS manages page cache     │ Zero-copy reads  │
│ Columnar layout         │ Read only needed columns  │ Less I/O         │
│ SIMD distance compute   │ 8+ float ops per cycle    │ 4-8x faster math │
│ Adaptive filtering      │ Pick best filter strategy │ No wasted work   │
│ Versioned storage       │ Append-only, no rewrites  │ Fast writes      │
└─────────────────────────┴───────────────────────────┴──────────────────┘

Typical performance:
  1M vectors (1536-dim), top-10 search:
  - Brute force:  ~500ms
  - IVF-PQ:       ~5ms    (95%+ recall)
  - DiskANN:      ~2ms    (98%+ recall)
```

## Common Patterns

### RAG Pipeline (Retrieval-Augmented Generation)

```python
import lancedb
from sentence_transformers import SentenceTransformer

# Create table with embeddings
db = lancedb.connect("./my_rag_db")
model = SentenceTransformer("all-MiniLM-L6-v2")

data = [
    {"text": "DuckDB is a columnar database", "vector": model.encode("DuckDB is a columnar database")},
    {"text": "Redis is an in-memory store",   "vector": model.encode("Redis is an in-memory store")},
]
table = db.create_table("docs", data)

# Search
query_vec = model.encode("fast analytics database")
results = table.search(query_vec).limit(5).to_pandas()
# → returns "DuckDB is a columnar database" as top result
```

### Multi-modal Search (Image + Text)

```python
# Store image embeddings alongside text and metadata
data = [
    {
        "image_uri": "s3://bucket/cat.jpg",
        "caption": "A cat sitting on a mat",
        "vector": clip_model.encode_image(image),  # 512-dim CLIP embedding
        "tags": ["animal", "indoor"],
        "timestamp": "2024-01-15",
    },
]
table = db.create_table("images", data)

# Search by text (encode text with same CLIP model)
text_vec = clip_model.encode_text("dog playing outside")
results = table.search(text_vec).where("'animal' IN tags").limit(10)
```

## Distance Metrics

```
LanceDB supports multiple distance functions for vector search:

┌────────────────┬──────────────────────────────────────┬─────────────────┐
│ Metric         │ Formula                              │ Use case        │
├────────────────┼──────────────────────────────────────┼─────────────────┤
│ L2 (Euclidean) │ √(Σ(aᵢ - bᵢ)²)                     │ General purpose │
│ Cosine         │ 1 - (a·b)/(|a|·|b|)                 │ Text embeddings │
│ Dot product    │ -Σ(aᵢ × bᵢ)                         │ Normalized vecs │
└────────────────┴──────────────────────────────────────┴─────────────────┘

Cosine similarity is most common for text embeddings (OpenAI, sentence-transformers)
because it measures angle between vectors, ignoring magnitude.
```

## When to Use What

```
Need traditional SQL analytics?             → DuckDB / ClickHouse
Need vector search (RAG, semantic search)?  → LanceDB / pgvector
Need vector search at huge scale?           → Milvus / Pinecone (managed)
Need embedded, no server?                   → LanceDB
Need vector + full-text hybrid search?      → Elasticsearch / Vespa
Small dataset (<100K vectors)?              → FAISS flat or brute force
Need multi-modal (images + text + vectors)? → LanceDB
```

## Key Internals to Know

| Component | Implementation | Why |
|-----------|---------------|-----|
| Storage format | Lance (custom columnar) | Random access + columnar scans + vector-native |
| ANN index | IVF-PQ, DiskANN | Partition + compress for sub-linear search |
| Versioning | Append-only manifests | Zero-copy MVCC, time travel |
| Compression | PQ for vectors, standard for scalars | 64x vector compression |
| I/O | Memory-mapped files | OS page cache, zero-copy |
| Language | Rust (core) + Python/JS bindings | Performance + ergonomics |
| Cloud storage | Native S3/GCS support | Lance files work on object storage |
| Quantization | Product Quantization, Scalar Quantization | Trade accuracy for speed/memory |
