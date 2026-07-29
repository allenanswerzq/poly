# Search Systems Deep Dive

## Overview

Search is the problem of finding relevant documents from a massive corpus given a user query. Modern search systems combine **information retrieval** (inverted indexes, BM25), **machine learning** (learning to rank), and **vector search** (semantic embeddings) into multi-stage ranking pipelines that serve results in <200ms.

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     Search: End-to-End Flow                             │
│                                                                         │
│   User types         Query              Multi-stage         Results     │
│   query        ──►  Processing   ──►   Retrieval +   ──►  Rendered     │
│   "best pizza"      (spelling,         Ranking              (SERP)     │
│                      expansion,        (<200ms)                         │
│                      intent)                                            │
│                                                                         │
│   User clicks/       Click logs        Offline              Models     │
│   ignores      ──►  collected   ──►   Training      ──►  Updated      │
│   results            (implicit         (LTR, embeddings)               │
│                       feedback)                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## 1. Inverted Index — The Foundation

### How It Works

```
Forward index (what a document contains):
  Doc 1: "the cat sat on the mat"
  Doc 2: "the dog sat on the log"
  Doc 3: "cats and dogs living together"

Inverted index (which documents contain a term):
  "cat"      → [Doc 1]
  "sat"      → [Doc 1, Doc 2]
  "dog"      → [Doc 2, Doc 3]
  "mat"      → [Doc 1]
  "the"      → [Doc 1, Doc 2]
  "log"      → [Doc 2]
  "cats"     → [Doc 3]
  "together" → [Doc 3]
  ...

Query "cat sat" → intersect posting lists:
  "cat" → [Doc 1]
  "sat" → [Doc 1, Doc 2]
  intersection → [Doc 1] ✓

This is O(n) in posting list length, not O(N) in corpus size.
With skip pointers, intersection is even faster (O(√n)).
```

### Posting List Structure

```
Each term maps to a sorted list of (doc_id, frequency, positions):

  "search" → [(doc_12, tf=3, pos=[5,18,42]),
               (doc_47, tf=1, pos=[7]),
               (doc_93, tf=5, pos=[1,12,28,35,50]),
               ...]

  doc_id:    which document (sorted for fast intersection)
  tf:        term frequency (how many times in this doc)
  positions: where in the document (for phrase queries, proximity)

Stored compressed on disk:
  - Delta encoding: [12, 47, 93] → [12, 35, 46] (store gaps)
  - Variable-byte or PForDelta compression
  - Block-based with skip pointers for fast seeking
```

### Building the Index (Indexing Pipeline)

```
Raw documents → Indexing pipeline → Inverted index

  ┌──────────────────────────────────────────────────────────────────┐
  │ Indexing Pipeline                                                 │
  │                                                                   │
  │  1. Document parsing (HTML → text, extract title/body/URL)       │
  │  2. Tokenization ("New York City" → ["new", "york", "city"])     │
  │  3. Normalization (lowercase, remove accents)                    │
  │  4. Stemming/Lemmatization ("running" → "run")                   │
  │  5. Stop word removal (optionally remove "the", "is", "at")     │
  │  6. Build posting lists (term → doc_id list)                     │
  │  7. Compute document statistics (doc length, term frequencies)   │
  │  8. Store document metadata (title, URL, snippet, timestamps)    │
  └──────────────────────────────────────────────────────────────────┘

At Google scale:
  - Crawl billions of pages
  - Build index shards distributed across thousands of machines
  - Each shard handles a slice of the document space
  - Index updated continuously (not batch)
```

## 2. Ranking — BM25 and TF-IDF

### TF-IDF (Term Frequency × Inverse Document Frequency)

```
The intuition:
  - A term that appears often in a document is important (TF)
  - A term that appears in few documents is distinctive (IDF)
  - "the" appears everywhere → low IDF → low score
  - "kubernetes" appears rarely → high IDF → high score

  TF(t, d) = count of term t in document d
  IDF(t) = log(N / df(t))    where N = total docs, df = docs containing t

  Score(t, d) = TF(t, d) × IDF(t)

  For a multi-term query: sum scores across all query terms.
```

### BM25 (Best Match 25) — The Standard

```
BM25 improves TF-IDF with saturation and length normalization:

  Score(q, d) = Σ  IDF(t) × (tf × (k1 + 1)) / (tf + k1 × (1 - b + b × |d|/avgdl))
               t∈q

  k1 = 1.2    (term frequency saturation — diminishing returns)
  b = 0.75    (length normalization — penalize long documents)
  |d|         (document length)
  avgdl       (average document length in corpus)

  Why k1 matters:
    TF-IDF: "pizza" appearing 100× scores 100× more than appearing once.
    BM25:   "pizza" appearing 100× scores ~5× more (saturation).
    This makes sense — 100 mentions doesn't make it 100× more relevant.

  Why b matters:
    A 10,000-word article mentioning "pizza" once is less relevant than
    a 100-word recipe mentioning "pizza" once. Length normalization.

BM25 is STILL the baseline for search engines. Elasticsearch, Solr, Lucene,
and most search systems use BM25 as the first-stage retriever.
```

## 3. Query Processing

```
User types: "runing shoees neer me"

┌──────────────────────────────────────────────────────────────────┐
│ Query Processing Pipeline                                        │
│                                                                   │
│ 1. Spell correction                                              │
│    "runing shoees neer" → "running shoes near"                   │
│    Methods: edit distance, noisy channel model, neural speller   │
│                                                                   │
│ 2. Tokenization + normalization                                  │
│    "running shoes near me" → ["running", "shoes", "near", "me"] │
│                                                                   │
│ 3. Query expansion / rewriting                                   │
│    Add synonyms: "running shoes" → "running shoes" OR            │
│                  "jogging sneakers" OR "athletic footwear"       │
│    Neural: encode query, find similar queries from logs          │
│                                                                   │
│ 4. Intent classification                                         │
│    "running shoes near me" → intent: LOCAL_SHOPPING              │
│    "what are running shoes" → intent: INFORMATIONAL              │
│    This changes what results to show (products vs articles)      │
│                                                                   │
│ 5. Location resolution                                           │
│    "near me" → user's lat/lon → filter by geo radius             │
│                                                                   │
│ 6. Named entity recognition                                      │
│    "Nike Air Max running shoes" → brand: Nike, product: Air Max  │
└──────────────────────────────────────────────────────────────────┘
```

## 4. Multi-Stage Ranking Pipeline

```
Same funnel idea as ad ranking, but for organic search results:

┌──────────────────────────────────────────────────────────────────┐
│  Stage 0: QUERY UNDERSTANDING                                    │
│  Spelling, expansion, intent, entity extraction                  │
│  Latency: ~10ms                                                  │
├──────────────────────────────────────────────────────────────────┤
│  Stage 1: RETRIEVAL (Candidate Generation)                       │
│  Corpus: billions of documents                                   │
│  Methods:                                                        │
│   • BM25 on inverted index (lexical match)                       │
│   • Dense retrieval: encode query + docs as vectors, ANN search  │
│   • Hybrid: combine BM25 + dense scores                          │
│  Output: ~10,000 candidates                                      │
│  Latency: ~20ms                                                  │
├──────────────────────────────────────────────────────────────────┤
│  Stage 2: LIGHTWEIGHT RANKING                                    │
│  Small model (distilled BERT, GBDT)                              │
│  Features: BM25 score, doc quality, freshness, click-through     │
│  Output: ~1,000 candidates                                       │
│  Latency: ~20ms                                                  │
├──────────────────────────────────────────────────────────────────┤
│  Stage 3: HEAVY RANKING (cross-encoder)                          │
│  BERT-based model: encode (query, document) together             │
│  Much more accurate but expensive (~1ms per document)            │
│  Output: ~100 ranked results                                     │
│  Latency: ~50-100ms                                              │
├──────────────────────────────────────────────────────────────────┤
│  Stage 4: RE-RANKING + BLENDING                                  │
│  Diversity (don't show 10 results from same site)                │
│  Freshness boost (recent news)                                   │
│  Personalization (user history)                                   │
│  Blend: web results + news + images + videos + ads               │
│  Output: final SERP (10 blue links + rich results)               │
│  Latency: ~10ms                                                  │
└──────────────────────────────────────────────────────────────────┘

Total latency budget: <200ms (Google's target is <500ms, p50 ~200ms)
```

## 5. Semantic Search (Dense Retrieval)

### The Problem with Keyword Search

```
Query: "how to fix a flat tire"
Doc:   "changing a punctured tyre — step-by-step guide"

BM25 score ≈ 0 (no word overlap: "fix"≠"changing", "flat"≠"punctured",
"tire"≠"tyre")

But semantically, this document is EXACTLY what the user wants.
```

### Dense Retrieval (Bi-Encoder)

```
Encode query and documents into dense vectors in the same embedding space.
Similar meaning → nearby vectors.

  Query: "how to fix a flat tire"     → vec_q = [0.12, -0.34, ..., 0.78]
  Doc:   "changing a punctured tyre"  → vec_d = [0.11, -0.32, ..., 0.80]

  cosine_similarity(vec_q, vec_d) = 0.97  ← high! Semantic match.

  ┌──────────────────────────────────────────────────────────────┐
  │  Bi-Encoder Architecture                                      │
  │                                                               │
  │  Query ──► [BERT encoder] ──► query vector ──┐                │
  │                                                │  dot product  │
  │  Doc   ──► [BERT encoder] ──► doc vector  ────┘  → score     │
  │                                                               │
  │  Documents encoded OFFLINE (index once, search many times)    │
  │  Query encoded ONLINE (~5ms for a single BERT forward pass)   │
  │                                                               │
  │  Search: ANN index (FAISS/HNSW) over pre-encoded doc vectors  │
  │  Find top-K nearest vectors → those are the relevant docs     │
  └──────────────────────────────────────────────────────────────┘

Models: DPR (Facebook), ColBERT, E5, GTE, Cohere Embed, OpenAI text-embedding
```

### Hybrid Search (BM25 + Dense)

```
Best results come from combining lexical (BM25) and semantic (dense):

  BM25 is great at:
    - Exact matches ("error code 0x80070005")
    - Rare terms (specific product names, IDs)
    - Efficiency (fast, no GPU needed)

  Dense is great at:
    - Semantic similarity (synonyms, paraphrases)
    - Cross-language search
    - Zero-shot (works on unseen terms)

  Combination: Reciprocal Rank Fusion (RRF)
    RRF_score(d) = Σ  1 / (k + rank_i(d))
                  i∈{BM25, dense}

    k = 60 (constant, dampens influence of high ranks)

  Or: learned linear combination
    score = α × BM25_score + (1-α) × dense_score
    α tuned on relevance labels.
```

### Cross-Encoder (Re-ranker)

```
Bi-encoder: encode query and doc SEPARATELY → fast but less accurate.
Cross-encoder: encode (query, doc) TOGETHER → slow but much more accurate.

  ┌────────────────────────────────────────────────┐
  │  Cross-Encoder                                  │
  │                                                 │
  │  Input: [CLS] query [SEP] document [SEP]        │
  │         ─────────────────────────────────       │
  │                       │                          │
  │                 [BERT/RoBERTa]                   │
  │                       │                          │
  │              relevance score (0-1)               │
  │                                                 │
  │  Query and document tokens ATTEND to each other  │
  │  → captures fine-grained interactions            │
  │  → "running" in query matches "jogging" in doc   │
  └────────────────────────────────────────────────┘

  Too slow for retrieval (must score every doc independently).
  Used for RE-RANKING top 100-1000 candidates from Stage 1.
  Cost: ~1ms per (query, doc) pair → 100 docs = ~100ms.
```

## 6. Learning to Rank (LTR)

```
Given a query and candidate documents, learn to ORDER them by relevance.

Input: feature vectors for each (query, document) pair
  - BM25 score
  - Dense retrieval score
  - PageRank of the document
  - Click-through rate (historical)
  - Freshness (how recent)
  - Document quality score
  - Query-document term overlap
  - URL depth, domain authority
  - ... 100+ features

Three approaches:

Pointwise: predict relevance score for each doc independently.
  Model: regression/classification. Loss: MSE or cross-entropy.
  Simple but ignores relative ordering.

Pairwise: predict which of two docs is more relevant.
  Model: RankNet (neural), LambdaMART (GBDT).
  Loss: probability that doc_i should rank above doc_j.
  Better for ranking quality.

Listwise: optimize entire ranking list directly.
  Model: LambdaRank, ApproxNDCG.
  Loss: directly optimizes NDCG/MAP over the full list.
  Best quality, hardest to train.

In practice:
  LambdaMART (XGBoost/LightGBM) is STILL the go-to for many systems.
  Google/Bing use deep learning (BERT-based rankers).
```

## 7. Relevance Metrics

```
┌──────────────┬──────────────────────────────────────────────────────┐
│ Metric       │ What it measures                                     │
├──────────────┼──────────────────────────────────────────────────────┤
│ Precision@K  │ Of top K results, how many are relevant?            │
│              │ P@10 = 7/10 = 0.70                                   │
│ Recall@K     │ Of all relevant docs, how many are in top K?        │
│              │ 7 of 20 relevant docs in top 10 → R@10 = 0.35      │
│ MAP          │ Mean Average Precision — average precision across    │
│              │ all relevant doc positions. Standard for ad-hoc IR. │
│ MRR          │ Mean Reciprocal Rank — 1/position of first relevant │
│              │ result. MRR = 1/3 if first relevant is at rank 3.   │
│ NDCG@K       │ Normalized Discounted Cumulative Gain               │
│              │ Considers graded relevance (not just yes/no).       │
│              │ Discounts relevance by log(position).               │
│              │ THE standard metric for search/LTR.                 │
└──────────────┴──────────────────────────────────────────────────────┘

NDCG example:
  Results: [highly_relevant, irrelevant, somewhat_relevant, ...]
  Gains:   [3,              0,           1,                  ...]
  DCG@3 = 3/log2(2) + 0/log2(3) + 1/log2(4) = 3 + 0 + 0.5 = 3.5
  Ideal:   [3, 1, 0] → IDCG@3 = 3/log2(2) + 1/log2(3) + 0 = 3.63
  NDCG@3 = 3.5 / 3.63 = 0.96
```

## 8. Index Serving Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                   Distributed Search Architecture                        │
│                                                                          │
│                         ┌───────────────┐                                │
│                         │  Query Router  │                                │
│                         │ (load balancer)│                                │
│                         └───────┬───────┘                                │
│                                 │ fan-out query to all shards            │
│               ┌─────────────────┼─────────────────┐                      │
│               ▼                 ▼                 ▼                      │
│        ┌──────────┐      ┌──────────┐      ┌──────────┐                 │
│        │ Shard 0  │      │ Shard 1  │      │ Shard 2  │                 │
│        │ docs 0-  │      │ docs 1M- │      │ docs 2M- │                 │
│        │ 999,999  │      │ 1,999,999│      │ 2,999,999│                 │
│        │          │      │          │      │          │                 │
│        │ inverted │      │ inverted │      │ inverted │                 │
│        │ index    │      │ index    │      │ index    │                 │
│        │ + dense  │      │ + dense  │      │ + dense  │                 │
│        │ vectors  │      │ vectors  │      │ vectors  │                 │
│        └────┬─────┘      └────┬─────┘      └────┬─────┘                 │
│             │                 │                 │                        │
│             └─────────────────┼─────────────────┘                        │
│                               ▼                                          │
│                        ┌──────────────┐                                  │
│                        │   Merger     │  merge top-K from each shard     │
│                        │              │  global re-rank                   │
│                        └──────────────┘                                  │
│                                                                          │
│  Document partitioning:                                                  │
│    By document (each shard has different docs) — most common             │
│    By term (each shard has different terms) — rare, used at Google       │
│                                                                          │
│  Each shard has replicas for availability + load distribution.          │
│  Query hits ALL shards (scatter-gather pattern).                        │
│  Latency = max(shard latencies) → tail latency matters.                │
└─────────────────────────────────────────────────────────────────────────┘
```

## 9. Autocomplete / Typeahead

```
User types: "how to m..."

Must return suggestions in <50ms (as user types each keystroke).

Data structure: Trie with frequency scores at each node.

                    h
                    │
                    o
                    │
                    w
                   ╱ ╲
                  t    ...
                  │
                  o
                 ╱ ╲
                m    ...
               ╱ ╲
             "make" (score: 500)
             "mix"  (score: 200)

Pre-computed top-K completions at each trie node.
Stored in memory (tries are ~1-5 GB for web-scale query logs).

Personalized: blend global popularity with user's search history.
Real-time: trending queries (earthquake, breaking news) boosted.
```

## 10. Snippet Generation

```
Query: "how to train a neural network"
Document: 1000-word article about deep learning

Snippet must:
  1. Contain the query terms (highlighted)
  2. Be ~160 characters (fits in search result)
  3. Be the most relevant passage (not just the beginning)

Methods:
  - Extract sentences containing most query terms
  - Score by BM25 at passage level (not document level)
  - Neural: use extractive QA model to find best passage
  - Highlight query terms in bold

Example output:
  "To **train a neural network**, start by preparing your dataset,
   defining the model architecture, and choosing a loss function..."
```

## 11. Freshness & Real-Time Indexing

```
Breaking news: user searches "earthquake" → must show results from 5 min ago.

Two-tier index:
  ┌────────────────┐     ┌────────────────┐
  │ Main Index     │     │ Real-Time Index │
  │ (billions of   │     │ (last few hours │
  │  documents,    │     │  of new/updated │
  │  rebuilt daily)│     │  documents)     │
  └───────┬────────┘     └───────┬────────┘
          │                       │
          └───────────┬───────────┘
                      ▼
               Merge results
               (boost fresh content for time-sensitive queries)

Real-time index:
  - Documents indexed within seconds of crawl/publish
  - In-memory inverted index (smaller, frequently rebuilt)
  - Merged into main index periodically

Google Caffeine (2010): continuous crawl + index update
  replaced the old batch rebuild approach.
```

## System Architecture — Full Search Engine

```
┌─────────────────────────────────────────────────────────────────────────┐
│                   Full Search Engine Architecture                        │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Crawler / Ingestion                                      │            │
│  │  Web crawler → HTML parser → content extraction          │            │
│  │  OR: API ingestion (product catalog, knowledge base)     │            │
│  └────────────────────────────┬────────────────────────────┘            │
│                               ▼                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Indexing Pipeline                                        │            │
│  │  Tokenize → normalize → build inverted index             │            │
│  │  Encode documents → build vector index (HNSW/IVF)        │            │
│  │  Extract metadata (title, URL, dates, entities)          │            │
│  │  Compute document quality signals (PageRank, spam score) │            │
│  └────────────────────────────┬────────────────────────────┘            │
│                               ▼                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Serving (Online, <200ms)                                 │            │
│  │                                                          │            │
│  │  Query → Understanding → Retrieval (BM25 + dense) →      │            │
│  │  Lightweight rank → Heavy rank (BERT) → Re-rank →        │            │
│  │  Snippet generation → Blend (web + news + images) →      │            │
│  │  SERP rendered                                           │            │
│  └─────────────────────────────────────────────────────────┘            │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────┐            │
│  │ Feedback Loop                                            │            │
│  │  Click logs → implicit relevance labels                  │            │
│  │  → Train LTR models, fine-tune embeddings                │            │
│  │  → Update query suggestions, spell correction            │            │
│  └─────────────────────────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────────────────┘
```

## Key Technologies

| Component | Common choices |
|-----------|---------------|
| Inverted index | Lucene (Elasticsearch/Solr), Tantivy (Rust), custom |
| Vector index | FAISS, HNSW (hnswlib), ScaNN, Vespa |
| Embedding model | E5, GTE, ColBERT, Cohere Embed, OpenAI |
| Re-ranker | Cross-encoder (BERT/RoBERTa), Cohere Rerank |
| LTR | LambdaMART (LightGBM/XGBoost), neural rankers |
| Serving | Elasticsearch, Vespa, Meilisearch, Typesense |
| Crawler | Scrapy, Colly (Go), custom distributed crawler |

## Numbers to Know

```
Google:        ~8.5 billion searches/day, index of hundreds of billions of pages
Elasticsearch: commonly serves <50ms at p99 for millions of documents
Lucene:        can index ~10K-100K documents/sec per node
BM25:          can score millions of documents in <10ms (inverted index)
BERT re-rank:  ~1ms per (query, doc) pair on GPU
Dense retrieval: ~5ms to encode query + ~10ms ANN search over 1M vectors
Autocomplete:  must respond in <50ms (every keystroke)
```

## Key Papers

| Paper | Year | Contribution |
|-------|------|-------------|
| PageRank (Google) | 1998 | Link-based document quality signal |
| BM25 (Robertson) | 1994 | Standard probabilistic retrieval function |
| LambdaMART | 2010 | GBDT-based learning to rank |
| Word2Vec | 2013 | Dense word representations |
| BERT (Google) | 2018 | Contextualized representations, revolutionized NLU |
| DPR (Facebook) | 2020 | Dense Passage Retrieval with bi-encoders |
| ColBERT | 2020 | Late interaction — efficient and accurate |
| E5 / GTE | 2023 | State-of-the-art text embeddings |

---

## Famous Systems — How They Work Internally

### Google Search

```
The most sophisticated search engine in the world.
~8.5 billion searches/day, index of hundreds of billions of pages.

How a Google search query is served:

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Google Search Serving                             │
  │                                                                       │
  │  User types "best restaurants in SF"                                  │
  │       │                                                               │
  │       ▼                                                               │
  │  ┌──────────────────┐                                                │
  │  │ Web Server        │  Route to nearest data center                  │
  │  │ (GWS)            │  Parse query, check cache                      │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Query Processing  │                                                │
  │  │                   │  Spell correction ("resturants" → "restaurants")│
  │  │                   │  Query expansion (synonyms, related terms)     │
  │  │                   │  Intent: LOCAL + RESTAURANT                    │
  │  │                   │  Entity detection: "SF" = San Francisco       │
  │  │                   │  Language detection                            │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Index Serving     │  Index is sharded across thousands of machines │
  │  │ (Scatter-Gather)  │                                                │
  │  │                   │  Fan out to ALL index shards simultaneously   │
  │  │                   │  Each shard: BM25 + signals → top K docs      │
  │  │                   │  Merge results from all shards               │
  │  │                   │  Latency = max(shard latencies)               │
  │  │                   │                                                │
  │  │  Two index tiers:                                                  │
  │  │   Tier 1: "base" index (all of the web, less frequently queried)  │
  │  │   Tier 2: "realtime" index (freshly crawled, breaking news)       │
  │  │   Most queries only hit Tier 1 (pre-computed top results)         │
  │  │   News/trending queries also search Tier 2                        │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Ranking Stack     │  Multi-stage, evolved over 25 years:           │
  │  │                   │                                                │
  │  │  Phase 1 (2000s): PageRank + BM25 + hand-tuned signals           │
  │  │    PageRank: score = Σ PageRank(linking_page) / outlinks         │
  │  │    Web spam detection (link farms, keyword stuffing)              │
  │  │    200+ ranking signals (domain age, HTTPS, mobile-friendly)     │
  │  │                                                                    │
  │  │  Phase 2 (2015): RankBrain                                        │
  │  │    ML model to handle unseen/ambiguous queries (~15% of queries)  │
  │  │    Word2Vec-style embeddings for query-document matching          │
  │  │    First time ML was a core ranking signal                        │
  │  │                                                                    │
  │  │  Phase 3 (2019): BERT                                             │
  │  │    Contextualized understanding of query AND document             │
  │  │    "parking on a hill with no curb" — BERT understands "no"       │
  │  │    Applied to 100% of English queries (now all languages)         │
  │  │    Used as re-ranker on top candidates (too expensive for all)    │
  │  │                                                                    │
  │  │  Phase 4 (2021): MUM (Multitask Unified Model)                    │
  │  │    1000x more powerful than BERT                                   │
  │  │    Multimodal (understands text AND images)                       │
  │  │    Multilingual (transfer knowledge across 75 languages)          │
  │  │    Can answer complex questions needing multi-step reasoning      │
  │  │                                                                    │
  │  │  Phase 5 (2024+): AI Overviews (Gemini-powered)                   │
  │  │    LLM generates summary from multiple sources                    │
  │  │    Grounded in search results (RAG-like)                          │
  │  │    Shown above organic results for informational queries          │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Result Assembly   │  Blend: organic + ads + Knowledge Panel +      │
  │  │ (SERP)           │  images + videos + news + People Also Ask +    │
  │  │                   │  featured snippets + local pack + shopping     │
  │  │                   │  Each vertical has its own ranking system      │
  │  │                   │  "Universal Search" merges them                │
  │  └──────────────────┘                                                │
  └──────────────────────────────────────────────────────────────────────┘

Infrastructure:
  • Googlebot: crawls the web continuously (Caffeine architecture, 2010)
  • Index updated in minutes for important sites, hours-days for long tail
  • Thousands of experiments/year (A/B tests on search quality)
  • ~10,000+ search quality raters (human evaluation)
  • Serving from 30+ data centers worldwide
  • Tail latency optimized: hedged requests, speculative execution

PageRank internals:
  Model the web as a directed graph. PageRank simulates a random surfer:
    PR(A) = (1-d)/N + d × Σ PR(T)/L(T)    for all pages T linking to A
    d = 0.85 (damping factor — 15% chance of jumping to random page)
    Solved iteratively (power iteration) across billions of pages.
    Now one of hundreds of signals, not the dominant one.
```

### Elasticsearch (and Apache Lucene)

```
The most widely-used search engine for application search.
Powers: Wikipedia, GitHub code search, Uber, Netflix, eBay.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                  Elasticsearch Architecture                           │
  │                                                                       │
  │  Core: built on Apache Lucene (Java search library by Doug Cutting)  │
  │  Elasticsearch = Lucene + distribution + REST API + management       │
  │                                                                       │
  │  ┌────────────────────────────────────────────────────────┐          │
  │  │ Cluster                                                 │          │
  │  │                                                         │          │
  │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │          │
  │  │  │ Node 1       │  │ Node 2       │  │ Node 3       │    │          │
  │  │  │              │  │              │  │              │    │          │
  │  │  │ Shard 0 (P)  │  │ Shard 0 (R)  │  │ Shard 1 (R)  │    │          │
  │  │  │ Shard 1 (P)  │  │ Shard 2 (P)  │  │ Shard 2 (R)  │    │          │
  │  │  │              │  │              │  │              │    │          │
  │  │  └─────────────┘  └─────────────┘  └─────────────┘    │          │
  │  │  P = primary shard, R = replica shard                   │          │
  │  │                                                         │          │
  │  │  Each shard = one Lucene index (inverted index + doc store)│      │
  │  │  Writes → primary shard → replicated to replica shards   │          │
  │  │  Reads → any shard (primary or replica)                  │          │
  │  └────────────────────────────────────────────────────────┘          │
  │                                                                       │
  │  Lucene internals:                                                    │
  │   ┌──────────────────────────────────────────┐                       │
  │   │ Lucene Index (one Elasticsearch shard)    │                       │
  │   │                                           │                       │
  │   │  ┌──────────┐ ┌──────────┐ ┌──────────┐  │                       │
  │   │  │ Segment 0│ │ Segment 1│ │ Segment 2│  │                       │
  │   │  │(immutable)│ │(immutable)│ │(immutable)│  │                       │
  │   │  │          │ │          │ │          │  │                       │
  │   │  │ inverted │ │ inverted │ │ inverted │  │                       │
  │   │  │ index    │ │ index    │ │ index    │  │                       │
  │   │  │ stored   │ │ stored   │ │ stored   │  │                       │
  │   │  │ fields   │ │ fields   │ │ fields   │  │                       │
  │   │  │ doc vals │ │ doc vals │ │ doc vals │  │                       │
  │   │  └──────────┘ └──────────┘ └──────────┘  │                       │
  │   │                                           │                       │
  │   │  Segments are IMMUTABLE (write-once).     │                       │
  │   │  New docs → new segment.                  │                       │
  │   │  Background merge: many small → fewer big │                       │
  │   │  Deletes: mark deleted, filter at read.   │                       │
  │   └──────────────────────────────────────────┘                       │
  │                                                                       │
  │  Write path:                                                          │
  │   Document → in-memory buffer → refresh (default 1s) → new segment  │
  │   → translog (WAL for crash recovery) → flush to disk → segment     │
  │   Near-real-time: new docs searchable within 1 second.               │
  │                                                                       │
  │  Query execution:                                                     │
  │   1. Parse query (query DSL → Lucene query)                          │
  │   2. Coordinate node fans out to all relevant shards                 │
  │   3. Each shard searches all segments, merges results                │
  │   4. Coordinate node merges shard results (top-K merge)              │
  │   5. Fetch phase: retrieve full documents for top results            │
  │                                                                       │
  │  Key features:                                                        │
  │   • Full-text search (BM25, analyzers, tokenizers)                   │
  │   • Aggregations (like SQL GROUP BY but more powerful)               │
  │   • Nested/parent-child documents (denormalized joins)               │
  │   • Geo search (geohash-based)                                       │
  │   • kNN vector search (HNSW, since v8.0)                             │
  │   • Cross-cluster search (federated queries)                         │
  │                                                                       │
  │  Performance tuning:                                                  │
  │   • Shard sizing: 10-50 GB per shard (sweet spot)                    │
  │   • Refresh interval: 1s default, increase for write-heavy           │
  │   • Bulk indexing: batch writes for 10x throughput                   │
  │   • Force merge: reduce segments for read-heavy indexes              │
  │   • Circuit breakers: prevent OOM from expensive queries             │
  └──────────────────────────────────────────────────────────────────────┘
```

### Vespa (Yahoo → Verizon Media → open-sourced)

```
Purpose-built for search + recommendation + ad serving at scale.
Used at Yahoo/Verizon for decades. Now open-source.

What makes Vespa different from Elasticsearch:

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Vespa Architecture                                │
  │                                                                       │
  │  Unlike Elasticsearch, Vespa is designed for:                        │
  │   • Structured + unstructured data in the same query                  │
  │   • ML model evaluation INSIDE the search engine                     │
  │   • Real-time updates (true real-time, not near-real-time)           │
  │   • Hybrid search: BM25 + vectors + filters in one query            │
  │                                                                       │
  │  ┌──────────────────────────────────────────────────────────┐       │
  │  │ Container Layer (stateless)                                │       │
  │  │  Query processing, federation, ML model serving            │       │
  │  │  Can run ONNX models during query time                     │       │
  │  │  (cross-encoder re-ranking inside the engine!)             │       │
  │  └──────────────────┬─────────────────────────────────────┘       │
  │                     ▼                                              │
  │  ┌──────────────────────────────────────────────────────────┐       │
  │  │ Content Layer (stateful)                                   │       │
  │  │  Proton search core (C++, custom-built, not Lucene)        │       │
  │  │  • Inverted index for text                                 │       │
  │  │  • B-tree for structured attributes                        │       │
  │  │  • HNSW for vector search                                  │       │
  │  │  • All three combined in a single query                    │       │
  │  │                                                            │       │
  │  │  Partial updates: update fields without reindexing         │       │
  │  │  (ES requires full document reindex for updates)           │       │
  │  └──────────────────────────────────────────────────────────┘       │
  │                                                                       │
  │  Ranking expressions (evaluated per document):                       │
  │   rank-profile my_model {                                            │
  │     first-phase {                                                     │
  │       expression: bm25(title) + bm25(body)                           │
  │     }                                                                 │
  │     second-phase {                                                    │
  │       expression: onnx(my_bert_model)   ← ML model in engine!       │
  │       rerank-count: 100                                              │
  │     }                                                                 │
  │   }                                                                   │
  │                                                                       │
  │  Used at: Yahoo Search, Yahoo Mail, Flickr, Spotify (podcast search) │
  └──────────────────────────────────────────────────────────────────────┘

Vespa vs Elasticsearch:
  ┌─────────────────┬───────────────────────┬───────────────────────┐
  │                 │ Elasticsearch          │ Vespa                  │
  ├─────────────────┼───────────────────────┼───────────────────────┤
  │ Core            │ Lucene (Java)          │ Proton (C++)           │
  │ Updates         │ Reindex full doc       │ Partial field update   │
  │ ML in engine    │ Limited (painless)     │ Full ONNX models       │
  │ Vector search   │ Added in v8 (HNSW)    │ Native from start      │
  │ Hybrid search   │ Separate queries       │ Single unified query   │
  │ Real-time       │ 1s refresh interval    │ True real-time writes  │
  │ Best for        │ Log analytics, simple  │ Production search +    │
  │                 │ search, observability  │ recommendations, ads   │
  └─────────────────┴───────────────────────┴───────────────────────┘
```

### Algolia (Hosted Search-as-a-Service)

```
Search API for websites and apps. ~17,000 customers.
Known for: speed (<50ms globally), typo tolerance, instant search.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Algolia Architecture                              │
  │                                                                       │
  │  Design philosophy: search MUST feel instant (<50ms, anywhere)       │
  │                                                                       │
  │  How they achieve speed:                                              │
  │                                                                       │
  │  1. Distributed Search Network (DSN)                                  │
  │     Index replicated to 70+ data centers worldwide                   │
  │     Query routed to nearest data center (like a CDN for search)      │
  │     User in Tokyo → Tokyo DC, user in Paris → Paris DC              │
  │                                                                       │
  │  2. In-memory index (RAM-first)                                      │
  │     Entire index lives in RAM (no disk seeks)                        │
  │     Custom data structures (not Lucene)                              │
  │     Proprietary trie-based index with bitmap intersection            │
  │                                                                       │
  │  3. Typo tolerance                                                    │
  │     "iphne" → "iphone" (edit distance computation on the fly)        │
  │     Pre-computed at index time: for each word, store all 1-edit      │
  │     distance variants → instant typo correction                      │
  │                                                                       │
  │  4. Instant search (search-as-you-type)                              │
  │     Prefix matching: "ipo" matches "ipod", "iphone"                  │
  │     Client sends query on every keystroke                            │
  │     Debounced + request cancellation on client side                  │
  │     Network: request + response < 50ms (including network RTT)       │
  │                                                                       │
  │  5. Tie-breaking ranking:                                             │
  │     Cascading criteria (not a single score):                         │
  │     1. Typo count (fewer typos = better)                             │
  │     2. Geo distance (if location provided)                           │
  │     3. Proximity (query words close together in result)              │
  │     4. Attribute importance (match in title > match in body)         │
  │     5. Exact match (exact > prefix match)                            │
  │     6. Custom ranking (popularity, rating, recency)                  │
  │     Each criterion is a tiebreaker for the previous one.             │
  │                                                                       │
  │  Limitations:                                                         │
  │   • No semantic search (purely lexical, typo-tolerant)               │
  │   • Index size limited (RAM-based → expensive for large corpora)     │
  │   • Not suitable for large-scale web search (designed for app search)│
  │   • Recently added AI Search (vector-based) to compete               │
  └──────────────────────────────────────────────────────────────────────┘
```

### Bing Search (Microsoft)

```
Second-largest web search engine (~10% market share).
Interesting because it reveals innovations Google doesn't publish about.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Bing Architecture Highlights                      │
  │                                                                       │
  │  Key engineering:                                                     │
  │                                                                       │
  │  1. FPGA-accelerated ranking (Project Catapult, 2014)                │
  │     Custom FPGAs in every server for ML inference                    │
  │     Each server has a Stratix V FPGA card                            │
  │     Free-form neural network evaluation on FPGA                      │
  │     2x ranking throughput with minimal latency increase              │
  │     → Later evolved into Azure's FPGA infrastructure                │
  │                                                                       │
  │  2. Turing models (precursor to GPT collaboration)                   │
  │     Turing-NLG: 17B parameter model for natural language generation  │
  │     Used for intelligent answers, captions, chat features            │
  │     → Led to Microsoft's investment in OpenAI                        │
  │                                                                       │
  │  3. Copilot integration (2023+)                                      │
  │     GPT-4 + Bing search index (RAG)                                  │
  │     Search → retrieve relevant pages → LLM generates answer         │
  │     Grounded in search results (citations)                           │
  │     Prometheus model: orchestrates search + LLM                      │
  │                                                                       │
  │  4. Entity understanding                                              │
  │     Satori knowledge graph (like Google Knowledge Graph)              │
  │     Entity linking: "Apple" → Apple Inc. or apple (fruit)?           │
  │     Knowledge panels, entity-based answers                           │
  │                                                                       │
  │  5. Multi-turn search                                                │
  │     Bing Chat / Copilot: conversational search with context          │
  │     "Show me hotels in Paris" → "Which ones have a pool?"            │
  │     System maintains conversation state + reformulates query         │
  │                                                                       │
  │  Architecture differences from Google:                                │
  │   • More aggressive use of neural ranking earlier                    │
  │   • FPGA acceleration (Google uses TPUs for similar purpose)         │
  │   • Tighter LLM integration (Copilot) before Google (AI Overviews)  │
  │   • Smaller index but more GPU inference budget per query            │
  └──────────────────────────────────────────────────────────────────────┘
```

### Perplexity AI (AI-Native Search)

```
A new breed of search: answer engine, not link engine.
No blue links — directly generates answers with citations.

  ┌──────────────────────────────────────────────────────────────────────┐
  │                     Perplexity Architecture                           │
  │                                                                       │
  │  Query: "What causes aurora borealis?"                                │
  │       │                                                               │
  │       ▼                                                               │
  │  ┌──────────────────┐                                                │
  │  │ Query Analysis    │  Classify: factual? current events? opinion?  │
  │  │                   │  Determine if web search needed               │
  │  │                   │  Rewrite query for retrieval                   │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Retrieval         │  Search own index + Bing API + other sources  │
  │  │                   │  Retrieve top ~20 web pages                    │
  │  │                   │  Extract relevant passages                     │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ LLM Generation    │  RAG: feed retrieved passages + query to LLM  │
  │  │                   │  Generate comprehensive answer                 │
  │  │                   │  Inline citations [1][2][3] for each claim     │
  │  │                   │  Streaming output (token by token)             │
  │  └──────┬───────────┘                                                │
  │         ▼                                                             │
  │  ┌──────────────────┐                                                │
  │  │ Follow-up         │  Suggest related questions                     │
  │  │                   │  Maintain conversation context                 │
  │  │                   │  Progressive refinement                        │
  │  └──────────────────┘                                                │
  │                                                                       │
  │  Key technical choices:                                                │
  │   • Own search index (built with web crawling) + Bing as fallback    │
  │   • Multiple LLM options (GPT-4, Claude, own fine-tuned models)      │
  │   • Citation grounding: each sentence linked to source               │
  │   • Pro Search: multi-step reasoning with multiple searches          │
  │   • Focus modes: Academic (papers), YouTube, Reddit, etc.            │
  │                                                                       │
  │  How it differs from Google AI Overviews:                              │
  │   • Answer-first (no blue links below)                                │
  │   • Explicit citations (numbered references to source pages)         │
  │   • Conversational (multi-turn follow-ups)                            │
  │   • No ads (subscription-based revenue model)                         │
  └──────────────────────────────────────────────────────────────────────┘
```

### Meilisearch / Typesense (Open-Source Alternatives)

```
Fast, easy-to-deploy search engines for application search.
Position themselves as open-source Algolia alternatives.

┌────────────────────┬─────────────────────────┬─────────────────────────┐
│                    │ Meilisearch              │ Typesense               │
├────────────────────┼─────────────────────────┼─────────────────────────┤
│ Language           │ Rust                     │ C++                     │
│ Index              │ Custom (LMDB-based)      │ Custom in-memory        │
│ Typo tolerance     │ Yes (DFA-based)          │ Yes                     │
│ Speed              │ <50ms typical            │ <50ms typical           │
│ Vector search      │ Yes (v1.3+)              │ Yes (built-in)          │
│ Faceting           │ Yes                      │ Yes                     │
│ Multi-tenancy      │ Via tenant tokens        │ Via API keys            │
│ Clustering (HA)    │ Experimental             │ Built-in Raft consensus │
│ Best for           │ Small-medium apps,       │ Production apps needing │
│                    │ developer experience     │ HA clustering           │
└────────────────────┴─────────────────────────┴─────────────────────────┘

Both: designed for sub-50ms response, typo tolerance out of the box,
simple REST API, instant search UIs. Neither is designed for web-scale search.

Meilisearch internals:
  • LMDB (Lightning Memory-mapped Database) for storage
  • BK-tree for typo tolerance (edit distance computation)
  • Bucket sort ranking (criteria-based, like Algolia)
  • Filterable attributes pre-indexed for fast filtering
  • Written in Rust → single binary, easy deployment

Typesense internals:
  • Everything in RAM (like Algolia)
  • Raft consensus for clustering (3-node minimum for HA)
  • Art (Adaptive Radix Trie) for prefix lookups
  • Built-in embeddings: auto-generate vectors using built-in models
  • Geo search with Haversine distance
```
