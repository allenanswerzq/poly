# Elasticsearch — How It Actually Works Inside

---

## 1. Why Elasticsearch Exists

```
The problem: SQL can't do full-text search.

  SELECT * FROM products WHERE title LIKE '%brown fox%';

  This scans EVERY row. No index helps. O(N) per query.
  No relevance ranking. No stemming ("running" ≠ "run").
  No fuzzy matching ("recieve" can't find "receive").

The solution: a completely different data structure — the inverted index.
Built on Apache Lucene (1999), wrapped in a distributed system (2010).

Who uses it:
  Wikipedia, GitHub (code search), Netflix (logging), Uber (geosearch),
  Stack Overflow, every company with a search bar.
```

---

## 2. The Inverted Index — The Core Data Structure

This is the single most important thing to understand. Everything else builds on it.

### 2.1 How a Book Index Works

Think about the index at the back of a textbook:

```
Traditional index (in a book):

  "algorithms" ......... pages 12, 45, 89, 201
  "binary tree" ........ pages 67, 92
  "hash table" ......... pages 34, 56, 78
  "sorting" ............ pages 12, 23, 45

  You DON'T read every page to find "hash table".
  You go to the index, look up the term, get the page numbers.
  That's O(log N) lookup instead of O(N) scan.
```

Elasticsearch does the **exact same thing**, but for documents instead of pages.

### 2.2 Building the Inverted Index — Step by Step

Let's index three product descriptions:

```
Step 1: You have documents.

  Doc 1: "Fast running shoes for men"
  Doc 2: "Men's lightweight running jacket"
  Doc 3: "Fast drying swim shorts for men"

Step 2: Each document goes through the ANALYZER pipeline.

  "Fast running shoes for men"
    │
    ├─ Tokenize (split on whitespace/punctuation)
    │    → ["Fast", "running", "shoes", "for", "men"]
    │
    ├─ Lowercase
    │    → ["fast", "running", "shoes", "for", "men"]
    │
    ├─ Stem (reduce to root form)
    │    → ["fast", "run", "shoe", "for", "men"]
    │
    └─ Remove stop words ("for" is meaningless)
         → ["fast", "run", "shoe", "men"]

  Same for Doc 2: → ["men", "lightweight", "run", "jacket"]
  Same for Doc 3: → ["fast", "dri", "swim", "short", "men"]

  "running" → "run" (stemming means searching "running" will find "run" too!)
  "drying"  → "dri" (Porter stemmer)

Step 3: Build the inverted index — flip the relationship.

  Instead of "document → words", store "word → documents":

  ┌───────────────┬──────────────┬─────────────────────────────┐
  │ Term          │ Doc Freq (df)│ Postings List               │
  ├───────────────┼──────────────┼─────────────────────────────┤
  │ fast          │ 2            │ Doc1:pos[0], Doc3:pos[0]    │
  │ run           │ 2            │ Doc1:pos[1], Doc2:pos[2]    │
  │ shoe          │ 1            │ Doc1:pos[2]                 │
  │ men           │ 3            │ Doc1:pos[3], Doc2:pos[0],   │
  │               │              │ Doc3:pos[4]                 │
  │ lightweight   │ 1            │ Doc2:pos[1]                 │
  │ jacket        │ 1            │ Doc2:pos[3]                 │
  │ dri           │ 1            │ Doc3:pos[1]                 │
  │ swim          │ 1            │ Doc3:pos[2]                 │
  │ short         │ 1            │ Doc3:pos[3]                 │
  └───────────────┴──────────────┴─────────────────────────────┘

  The "postings list" stores: which documents, at which positions.

Step 4: Terms are stored in a SORTED structure (like a B-tree or FST).

  This means looking up any term is O(log N), not O(N).
```

### 2.3 How a Search Query Uses the Inverted Index

```
Query: "running shoes"

Step 1: Analyze the query with the SAME analyzer as indexing.
  "running shoes"
    → tokenize → ["running", "shoes"]
    → lowercase → ["running", "shoes"]
    → stem      → ["run", "shoe"]

  (This is why the same analyzer must be used for indexing AND searching.
   If you indexed with stemming but searched without it,
   "run" in the index would never match "running" in the query.)

Step 2: Look up each term in the inverted index.
  "run"  → postings: [Doc1, Doc2]
  "shoe" → postings: [Doc1]

Step 3: Combine results.
  For a boolean AND: intersection → [Doc1]
  For a boolean OR:  union       → [Doc1, Doc2]

  Default in Elasticsearch: OR (find documents with ANY of the terms).
  Use "operator": "and" for AND behavior.

Step 4: Score each matching document (BM25) to rank by relevance.
  Doc1 has BOTH terms → higher score
  Doc2 has only "run" → lower score

Step 5: Return top-K results sorted by score.

THE KEY INSIGHT: We never scanned the documents themselves.
  We went: query terms → index lookup → document IDs → done.
  With 100 million documents, this is still milliseconds.
```

### 2.4 What the Inverted Index Actually Stores on Disk

```
The inverted index has several sub-structures:

  1. Term Dictionary
     ┌────────────────────────────────────────────────┐
     │  Sorted list of all unique terms.              │
     │  Stored as a Finite State Transducer (FST)     │
     │  — a compressed trie that maps term → pointer  │
     │  to the postings list on disk.                 │
     │                                                │
     │  FST is small enough to live in memory.        │
     │  So term lookup = in-memory FST traversal      │
     │  → O(length of term), basically instant.       │
     └────────────────────────────────────────────────┘

  2. Postings List (per term)
     ┌────────────────────────────────────────────────┐
     │  Delta-encoded, compressed list of doc IDs.    │
     │                                                │
     │  Raw:    [1, 5, 8, 12, 100, 103]              │
     │  Deltas: [1, 4, 3,  4,  88,   3]              │
     │                                                │
     │  Small numbers compress better (variable-byte  │
     │  or PFOR encoding). A million doc IDs might    │
     │  compress to a few KB.                         │
     └────────────────────────────────────────────────┘

  3. Term Frequencies
     ┌────────────────────────────────────────────────┐
     │  How many times each term appears in each doc. │
     │  Needed for BM25 scoring.                      │
     └────────────────────────────────────────────────┘

  4. Positions (optional, for phrase queries)
     ┌────────────────────────────────────────────────┐
     │  Where in the document each term appears.      │
     │  "quick brown fox" as a PHRASE query needs     │
     │  positions to verify the words are adjacent.   │
     └────────────────────────────────────────────────┘

  5. Norms
     ┌────────────────────────────────────────────────┐
     │  Document length factors for scoring.          │
     │  Short documents with a term match are more    │
     │  relevant than long documents with a match.    │
     └────────────────────────────────────────────────┘

  6. Stored Fields (the _source)
     ┌────────────────────────────────────────────────┐
     │  The original JSON document, compressed.       │
     │  NOT used for searching — only returned in     │
     │  results. The inverted index is for searching, │
     │  stored fields are for displaying.             │
     └────────────────────────────────────────────────┘

  7. Doc Values (columnar store for sorting/aggregations)
     ┌────────────────────────────────────────────────┐
     │  Column-oriented storage for numeric/keyword   │
     │  fields. Used when you need to sort by price   │
     │  or aggregate by category.                     │
     │                                                │
     │  Inverted index: term → docs (good for search) │
     │  Doc values:     doc → values (good for sort)  │
     └────────────────────────────────────────────────┘
```

---

## 3. Lucene Segments — The Write Model

This is where people get confused. Elasticsearch doesn't just "have" an inverted
index — it has a **collection of immutable segments**, each containing its own
inverted index.

### 3.1 Why Segments?

```
The problem: inverted indexes are expensive to update.

  If you have a sorted term dictionary with 10 million entries and you
  add a new document with a new term, you'd need to INSERT into the
  middle of a sorted structure → multiple disk seeks, rewriting data.

  This is too slow for a system that ingests thousands of docs/sec.

The solution: NEVER modify an existing index. Instead:
  1. Buffer new documents in memory
  2. Periodically write them as a NEW, small, immutable segment
  3. Searches query ALL segments and merge results
  4. Background process merges small segments into larger ones

  This is the same idea as LSM-trees (used in LevelDB, RocksDB, Cassandra).
```

### 3.2 The Lifecycle of a Document

```
You index a document: PUT /products/_doc/1 {"title": "running shoes"}

  ┌─────────────────────────────────────────────────────────────────┐
  │                                                                 │
  │  Step 1: WRITE TO TRANSLOG (write-ahead log)                   │
  │                                                                 │
  │    The document is immediately written to a transaction log     │
  │    on disk. This is an append-only file — fast sequential I/O. │
  │    Even if the node crashes RIGHT NOW, the document is safe.   │
  │                                                                 │
  │  Step 2: ADD TO IN-MEMORY BUFFER                               │
  │                                                                 │
  │    The document is also added to an in-memory indexing buffer.  │
  │    It is NOT searchable yet. No inverted index entry exists.   │
  │                                                                 │
  │    ┌──────────────────────────┐                                │
  │    │   In-memory buffer       │    Not searchable!             │
  │    │   doc1, doc2, doc3, ...  │                                │
  │    └──────────────────────────┘                                │
  │                                                                 │
  │  Step 3: REFRESH (every 1 second by default)                   │
  │                                                                 │
  │    The in-memory buffer is written to a new Lucene SEGMENT     │
  │    in the filesystem cache (not yet fsync'd to disk).          │
  │    The segment has its own mini inverted index.                │
  │    NOW the documents are searchable.                           │
  │                                                                 │
  │    ┌──────────────────────────┐                                │
  │    │   Segment 4 (new)       │    ← searchable now!           │
  │    │   mini inverted index   │                                 │
  │    │   doc1, doc2, doc3      │                                 │
  │    └──────────────────────────┘                                │
  │                                                                 │
  │    This is "near real-time" search — up to 1 second delay.    │
  │                                                                 │
  │  Step 4: FLUSH (periodically, or when translog gets big)       │
  │                                                                 │
  │    All segments in the filesystem cache are fsync'd to disk.   │
  │    The translog is cleared (those docs are now safely in       │
  │    segments on disk).                                          │
  │                                                                 │
  │  Step 5: MERGE (background)                                    │
  │                                                                 │
  │    Too many small segments = slow search (must query each one).│
  │    A background thread merges small segments into larger ones:  │
  │                                                                 │
  │    Seg0 ─┐                                                      │
  │    Seg1 ─┼──→ Merged Seg A                                     │
  │    Seg2 ─┘                                                      │
  │    Seg3 ─┐                                                      │
  │    Seg4 ─┼──→ Merged Seg B                                     │
  │    Seg5 ─┘                                                      │
  │                                                                 │
  │    After merge: old segments are deleted.                       │
  │    Merging also REMOVES deleted documents (see below).         │
  │                                                                 │
  └─────────────────────────────────────────────────────────────────┘
```

### 3.3 How Deletes Work (They Don't — Until Merge)

```
Segments are IMMUTABLE. You cannot modify or delete from a segment.

  DELETE /products/_doc/1

  What actually happens:
  1. The doc ID is added to a ".del" bitset file for that segment.
  2. At search time, docs in the .del set are filtered out of results.
  3. The bytes are still on disk, the inverted index still has the terms.
  4. Only during MERGE are deleted documents actually removed.

  UPDATE is the same: delete old version + index new version.

  This means:
  - Frequent updates = lots of deleted-but-not-removed docs = wasted space
  - Segment merging is what actually reclaims space
  - Force-merging (_forcemerge) can help for read-heavy indexes
```

### 3.4 Why This Design Is Fast

```
Writes:                               Reads:
  ┌─────────────────────────────┐      ┌──────────────────────────────┐
  │ 1. Append to translog       │      │ 1. Query hits ALL segments   │
  │    (sequential write, fast) │      │ 2. Each segment has its own  │
  │                             │      │    inverted index             │
  │ 2. Buffer in memory         │      │ 3. Results merged + ranked   │
  │    (just RAM, fast)         │      │                              │
  │                             │      │ More segments = slower search │
  │ 3. Periodic flush to segment│      │ Merging keeps segment count  │
  │    (sequential write, fast) │      │ low                          │
  └─────────────────────────────┘      └──────────────────────────────┘

  No random I/O for writes (unlike B-trees).
  Reads are parallel across segments.
  Immutability = segments can be cached aggressively by the OS.
```

---

## 4. BM25 — How Relevance Scoring Works

Every full-text search returns results ranked by a **score**. The score says
"how relevant is this document to this query?" BM25 is the formula that
computes it. Let's break it apart so it makes intuitive sense.

### 4.1 The Three Intuitions Behind BM25

```
Intuition 1: TERM FREQUENCY (TF)
  A document that mentions "elasticsearch" 5 times is probably more
  relevant to a query about "elasticsearch" than one that mentions it once.

  But 10 mentions isn't 10× more relevant than 1 mention.
  Returns diminish. BM25 uses a saturating function:

  TF score
  ▲
  │            ╭──────────── ← saturates (diminishing returns)
  │          ╱
  │        ╱
  │      ╱
  │    ╱
  │  ╱
  │╱
  └──────────────────────→ Term count in document

  Formula: tf_score = (freq * (k1 + 1)) / (freq + k1 * (1 - b + b * dl/avgdl))

  freq  = how many times the term appears in this document
  k1    = saturation parameter (default 1.2). Higher = slower saturation.
  b     = length normalization (default 0.75)
  dl    = document length (in terms)
  avgdl = average document length across all documents


Intuition 2: INVERSE DOCUMENT FREQUENCY (IDF)
  A term that appears in EVERY document (like "the") is useless for ranking.
  A term that appears in FEW documents (like "elasticsearch") is very useful.

  IDF(term) = log(1 + (N - df + 0.5) / (df + 0.5))

  N  = total documents in the index
  df = documents containing this term

  Term "the":             df = 1,000,000 out of 1,000,000 → IDF ≈ 0
  Term "elasticsearch":   df = 500 out of 1,000,000       → IDF ≈ 7.6

  Rare terms get high IDF. Common terms get low IDF.


Intuition 3: DOCUMENT LENGTH NORMALIZATION
  A 10-word document mentioning "shoes" once is probably about shoes.
  A 10,000-word document mentioning "shoes" once might just mention shoes
  in passing. Shorter documents with a match should score higher.

  This is controlled by the `b` parameter in BM25:
  b = 0: No length normalization
  b = 1: Full length normalization
  b = 0.75: Default — moderate normalization
```

### 4.2 Putting It Together — Full BM25 Walkthrough

```
Query: "running shoes"
Index has 10,000 documents. Average document length = 50 terms.

  Doc A: "Best running shoes for marathon training" (7 terms)
    "running" appears 1 time, "shoes" appears 1 time

  Doc B: "Complete guide to buying athletic equipment including running
          shoes, hiking boots, and cycling gear" (15 terms)
    "running" appears 1 time, "shoes" appears 1 time

Step 1: Compute IDF for each query term.
  "running": appears in 2,000 docs
    IDF = log(1 + (10000 - 2000 + 0.5) / (2000 + 0.5)) = log(1 + 4.0) ≈ 1.61

  "shoes": appears in 500 docs
    IDF = log(1 + (10000 - 500 + 0.5) / (500 + 0.5)) = log(1 + 19.0) ≈ 3.00

  "shoes" has HIGHER IDF → it's the more discriminating term.

Step 2: Compute TF component for each doc × each term.
  Using k1=1.2, b=0.75, avgdl=50.

  Doc A (dl=7):
    "running": freq=1
      tf = (1 * 2.2) / (1 + 1.2 * (1 - 0.75 + 0.75 * 7/50))
         = 2.2 / (1 + 1.2 * (0.25 + 0.105))
         = 2.2 / (1 + 0.426)
         = 2.2 / 1.426 = 1.54

    "shoes": freq=1, same calculation → tf = 1.54

  Doc B (dl=15):
    "running": freq=1
      tf = (1 * 2.2) / (1 + 1.2 * (1 - 0.75 + 0.75 * 15/50))
         = 2.2 / (1 + 1.2 * (0.25 + 0.225))
         = 2.2 / (1 + 0.57)
         = 2.2 / 1.57 = 1.40

    "shoes": freq=1, same → tf = 1.40

Step 3: Score = sum over query terms of (IDF × TF)

  Doc A: 1.61 × 1.54 + 3.00 × 1.54 = 2.48 + 4.62 = 7.10
  Doc B: 1.61 × 1.40 + 3.00 × 1.40 = 2.25 + 4.20 = 6.46

  Doc A scores higher because it's SHORTER (7 vs 15 terms).
  Same number of matches, but Doc A is "more about" shoes/running.

  The IDF ensures that "shoes" (rarer) contributes more than "running" (common).
  The TF saturation ensures that matching a term 10× isn't 10× better.
  Length normalization ensures short, focused docs beat long, tangential ones.
```

---

## 5. The Analyzer — How Text Gets Indexed and Searched

### 5.1 The Pipeline

When you index a document, the text fields go through an **analysis** pipeline
before being stored in the inverted index. When you search, the query goes through
the **same** pipeline.

```
Input: "The QUICK Brown Foxes jumped over 2 lazy dogs!"
         │
         ▼
┌─────────────────────┐
│  Character Filters   │  Strip HTML tags, replace & → "and", etc.
│  (optional)          │  → "The QUICK Brown Foxes jumped over 2 lazy dogs!"
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Tokenizer           │  Split text into tokens.
│  (one per analyzer)  │  Standard tokenizer splits on whitespace + punctuation:
│                      │  → ["The", "QUICK", "Brown", "Foxes", "jumped", "over",
│                      │     "2", "lazy", "dogs"]
└─────────┬───────────┘
          ▼
┌─────────────────────┐
│  Token Filters       │  Transform tokens one by one (in order):
│  (zero or more)      │
│                      │  1. lowercase:    → ["the", "quick", "brown", "foxes",
│                      │                      "jumped", "over", "2", "lazy", "dogs"]
│                      │
│                      │  2. stop words:   → ["quick", "brown", "foxes", "jumped",
│                      │     (remove "the",    "2", "lazy", "dogs"]
│                      │      "over")
│                      │
│                      │  3. stemmer:      → ["quick", "brown", "fox", "jump",
│                      │     (Porter/       "2", "lazi", "dog"]
│                      │      Snowball)
└─────────────────────┘

Final tokens stored in inverted index: ["quick", "brown", "fox", "jump", "2", "lazi", "dog"]

Now when someone searches for "foxes jumping", the SAME analyzer runs:
  "foxes jumping" → ["fox", "jump"] → matches!

If the analyzers were DIFFERENT between index and search time,
"fox" ≠ "foxes" and you'd get no match. This is a common bug.
```

### 5.2 Why "text" vs "keyword" Matters

```
Field type "text":
  Value: "Running Shoes"
  Analyzed → tokens: ["running", "shoes"]  (or ["run", "shoe"] with stemming)
  Stored in inverted index. Supports full-text search.
  But you CANNOT sort or aggregate on it (the original value is gone).

Field type "keyword":
  Value: "Running Shoes"
  NOT analyzed. Stored as-is: "Running Shoes"
  Supports exact match, sorting, aggregations.
  But searching for "running" won't match "Running Shoes".

That's why you often see BOTH on one field:

  "title": {
    "type": "text",             ← for full-text search
    "fields": {
      "keyword": {
        "type": "keyword"       ← for sorting, aggregations, exact match
      }
    }
  }

  Search:     match on "title" (analyzed)
  Sort:       sort on "title.keyword" (exact)
  Aggregate:  aggs on "title.keyword" (exact)
```

---

## 6. Distributed Architecture — How the Cluster Works

### 6.1 The Data Model

```
Cluster → Nodes → Indices → Shards → Segments → Documents

  ┌─────────────────── Cluster "production" ───────────────────┐
  │                                                             │
  │  Node 1            Node 2            Node 3                │
  │  (master+data)     (data)            (data)                │
  │                                                             │
  │  ┌──────────┐     ┌──────────┐     ┌──────────┐           │
  │  │ P0       │     │ P1       │     │ P2       │           │
  │  │ ┌──────┐ │     │ ┌──────┐ │     │ ┌──────┐ │           │
  │  │ │Seg 0 │ │     │ │Seg 0 │ │     │ │Seg 0 │ │           │
  │  │ │Seg 1 │ │     │ │Seg 1 │ │     │ │Seg 1 │ │           │
  │  │ │Seg 2 │ │     │ │Seg 2 │ │     │ └──────┘ │           │
  │  │ └──────┘ │     │ └──────┘ │     └──────────┘           │
  │  └──────────┘     └──────────┘                             │
  │                                                             │
  │  ┌──────────┐     ┌──────────┐     ┌──────────┐           │
  │  │ R1       │     │ R2       │     │ R0       │           │
  │  │ (replica │     │ (replica │     │ (replica │           │
  │  │  of P1)  │     │  of P2)  │     │  of P0)  │           │
  │  └──────────┘     └──────────┘     └──────────┘           │
  │                                                             │
  └─────────────────────────────────────────────────────────────┘

  Index "products" → 3 primary shards (P0, P1, P2)
                   → 1 replica each (R0, R1, R2)
  Each shard is a full Lucene index (with its own segments).
  Primary handles writes. Replica handles reads and failover.
```

### 6.2 How a Document Gets Routed to a Shard

```
When you index a document, which shard does it go to?

  shard_number = hash(_routing) % number_of_primary_shards

  Default _routing = document _id.

  Example: PUT /products/_doc/abc123
    hash("abc123") = 8472916
    8472916 % 3 = 1
    → Goes to shard P1

  THIS IS WHY YOU CANNOT CHANGE THE NUMBER OF PRIMARY SHARDS
  AFTER INDEX CREATION. If you change it, the hash mod changes,
  and documents are "lost" (routed to wrong shard on query).

  To change shard count, you must REINDEX into a new index.
```

### 6.3 How a Write (Index) Request Flows

```
Client: PUT /products/_doc/1 {"title": "running shoes", "price": 59.99}
  │
  ▼
┌──────────────────────────────────────────────────────────────────┐
│ Step 1: Request hits a COORDINATING NODE                        │
│   (any node can be a coordinator — it's whoever received the    │
│    HTTP request)                                                 │
│                                                                  │
│ Step 2: Coordinator computes target shard                        │
│   shard = hash("1") % 3 = 2 → primary shard P2 (on Node 3)    │
│                                                                  │
│ Step 3: Forward to primary shard                                 │
│   Coordinator → Node 3 (owns P2)                                │
│                                                                  │
│ Step 4: Primary shard indexes the document                       │
│   a. Write to translog (durability)                              │
│   b. Add to in-memory buffer (not searchable yet)                │
│   c. Return success to coordinator? NO — not yet.               │
│                                                                  │
│ Step 5: Replicate to replica shard(s)                            │
│   Primary P2 → sends document to R2 (on Node 2)                │
│   R2 indexes it (same translog + buffer process)                │
│   R2 acknowledges back to P2                                    │
│                                                                  │
│ Step 6: Primary acknowledges to coordinator                      │
│   Only after ALL in-sync replicas have confirmed.               │
│                                                                  │
│ Step 7: Coordinator returns 201 Created to client                │
│                                                                  │
│   Total path:                                                    │
│   Client → Coordinator → Primary → Replica(s) → Primary →      │
│   Coordinator → Client                                           │
│                                                                  │
│   The document is DURABLE (in translog) but NOT SEARCHABLE      │
│   until the next refresh (up to 1 second later).                │
└──────────────────────────────────────────────────────────────────┘
```

### 6.4 How a Search Request Flows

```
Client: GET /products/_search {"query": {"match": {"title": "running shoes"}}}
  │
  ▼
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│  ┌──────────── SCATTER PHASE (Query) ────────────┐              │
│  │                                                │              │
│  │  Coordinator needs results from ALL shards.    │              │
│  │  But it can query primary OR replica of each.  │              │
│  │                                                │              │
│  │  Coordinator sends query to:                   │              │
│  │    Shard 0: pick P0 (Node 1) or R0 (Node 3)  │              │
│  │    Shard 1: pick P1 (Node 2) or R1 (Node 1)  │              │
│  │    Shard 2: pick P2 (Node 3) or R2 (Node 2)  │              │
│  │                                                │              │
│  │  (Round-robin or adaptive: pick least-busy)    │              │
│  │                                                │              │
│  │  Each shard:                                   │              │
│  │    1. Runs the query on ALL its segments       │              │
│  │    2. Computes BM25 scores                     │              │
│  │    3. Returns top-K (doc_id, score) pairs      │              │
│  │       (NOT the full documents — just IDs)      │              │
│  │                                                │              │
│  └────────────────────────────────────────────────┘              │
│                          │                                       │
│                          ▼                                       │
│  ┌──────────── GATHER PHASE (Fetch) ─────────────┐              │
│  │                                                │              │
│  │  Coordinator receives top-K from each shard.   │              │
│  │  Merges and re-sorts globally.                 │              │
│  │  Picks the final top-K document IDs.           │              │
│  │                                                │              │
│  │  Then fetches the FULL documents (_source)     │              │
│  │  from only the shards that have those docs.    │              │
│  │                                                │              │
│  │  This is why it's called "scatter-gather":     │              │
│  │    Scatter → query to all shards (parallel)    │              │
│  │    Gather  → merge results + fetch docs        │              │
│  │                                                │              │
│  └────────────────────────────────────────────────┘              │
│                          │                                       │
│                          ▼                                       │
│  Return JSON response to client with scored results.             │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘

Performance implications:
  - More shards = more parallelism BUT more overhead (each shard does work)
  - Each shard returns top-K, so coordinator merges K × num_shards results
  - Deep pagination (from: 10000, size: 10) is expensive:
    each shard must return 10,010 results for the coordinator to merge
```

### 6.5 Node Roles

```
┌──────────────────┬────────────────────────────────────────────┐
│ Role             │ What it does                               │
├──────────────────┼────────────────────────────────────────────┤
│ Master           │ Manages cluster state: which nodes exist,  │
│                  │ which shards are where, index settings.    │
│                  │ Elected via quorum. Lightweight.           │
│                  │ Minimum 3 master-eligible nodes for HA.    │
├──────────────────┼────────────────────────────────────────────┤
│ Data             │ Stores data, executes search/index on its  │
│                  │ local shards. The muscle of the cluster.   │
├──────────────────┼────────────────────────────────────────────┤
│ Coordinating     │ Routes requests, merges results.           │
│ (default: all    │ Any node can coordinate. Dedicated ones    │
│  nodes can)      │ offload merge work from data nodes.        │
├──────────────────┼────────────────────────────────────────────┤
│ Ingest           │ Runs ingest pipelines (enrich, transform   │
│                  │ documents before indexing).                 │
└──────────────────┴────────────────────────────────────────────┘

Production setup:
  3 dedicated master nodes (small machines, no data)
  N data nodes (big machines, lots of disk and RAM)
  2+ coordinating-only nodes (behind a load balancer)
```

### 6.6 What Happens When a Node Dies

```
Node 2 crashes (holds P1 and R2):

  1. Master detects Node 2 is gone (no heartbeat for 30-90 seconds).

  2. Shards on Node 2:
     P1 (primary) → R1 on Node 1 is PROMOTED to primary.
                     Cluster is now functional but one replica short.
     R2 (replica) → Just lost. P2 on Node 3 still exists.

  3. Master triggers re-allocation:
     New replica of P1 → created on Node 3 (copy data from new primary R1→P1)
     New replica of P2 → created on Node 1

  4. Status transitions:
     GREEN  → all primaries + replicas allocated
     YELLOW → all primaries OK, some replicas missing (during recovery)
     RED    → some primaries missing (data loss or unavailable)

     Node 2 dies → cluster goes YELLOW → recovery → back to GREEN.

  If Node 2 comes back, it rejoins and its stale shards resync
  (only the delta, using the translog).
```

---

## 7. Query vs. Filter Context — A Critical Distinction

```
┌───────────────────────────────────────────────────────────────┐
│  QUERY context                    FILTER context              │
│                                                               │
│  "How well does this              "Does this document         │
│   document match?"                 match? Yes or no."         │
│                                                               │
│  Computes relevance SCORE.        Binary: include/exclude.    │
│  Uses BM25.                       NO scoring (faster).        │
│  Cannot be cached.                Results ARE cached.         │
│                                                               │
│  Use for: full-text search        Use for: exact filters      │
│           "search for shoes"              "price < 100"       │
│           "match this phrase"             "status = active"   │
│                                           "date > 2024-01-01"│
└───────────────────────────────────────────────────────────────┘

In practice, a well-structured query uses BOTH:

{
  "query": {
    "bool": {
      "must": [
        { "match": { "title": "running shoes" } }   ← QUERY (scored)
      ],
      "filter": [                                     ← FILTER (not scored, cached)
        { "term": { "brand": "nike" } },
        { "range": { "price": { "lte": 100 } } }
      ]
    }
  }
}

Why this matters for performance:
  - Filters are cached as bitsets: [1, 0, 1, 1, 0, 0, 1, ...]
  - A cached filter for "brand=nike" is a bitmap of matching doc IDs
  - Combining filters = bitwise AND of bitmaps → extremely fast
  - The expensive BM25 scoring only runs on the filtered subset
  - A query with 3 filters and 1 match clause:
    filter narrows 10M docs → 50K docs, then BM25 scores only 50K
```

---

## 8. Aggregations — The Analytics Engine

Aggregations are how Elasticsearch does GROUP BY, but much more powerful.

```
Example: E-commerce product search with facets.

{
  "query": { "match": { "title": "running shoes" } },
  "aggs": {
    "by_brand": {
      "terms": { "field": "brand.keyword", "size": 10 }
    },
    "price_ranges": {
      "range": {
        "field": "price",
        "ranges": [
          { "to": 50 },
          { "from": 50, "to": 100 },
          { "from": 100 }
        ]
      }
    },
    "avg_price": {
      "avg": { "field": "price" }
    }
  }
}

Response:
{
  "hits": { ... search results ... },
  "aggregations": {
    "by_brand": {
      "buckets": [
        { "key": "Nike",    "doc_count": 342 },
        { "key": "Adidas",  "doc_count": 287 },
        { "key": "Asics",   "doc_count": 156 }
      ]
    },
    "price_ranges": {
      "buckets": [
        { "key": "*-50.0",     "doc_count": 234 },
        { "key": "50.0-100.0", "doc_count": 412 },
        { "key": "100.0-*",    "doc_count": 139 }
      ]
    },
    "avg_price": { "value": 78.50 }
  }
}

This is one request that gives you:
  1. Search results (relevance-ranked)
  2. Brand facets (for the sidebar filter)
  3. Price range distribution
  4. Average price

How it works internally:
  - Aggregations use DOC VALUES (columnar storage), not the inverted index.
  - Doc values are stored per-field, per-segment: [50.0, 79.99, 42.0, ...]
  - Reading a column sequentially is fast (CPU cache-friendly).
  - This is why you can't aggregate on "text" fields — they're only in the
    inverted index, not in doc values. Use "keyword" for aggregations.
```

---

## 9. Near Real-Time Search — Why the 1-Second Delay

```
You index a document. It's NOT instantly searchable. Why?

Timeline:
  t=0.0s  Document indexed → translog + memory buffer
  t=0.0s  Search for it → NOT FOUND (not in any segment yet)

  t=1.0s  REFRESH happens → buffer written to new segment
  t=1.0s  Search for it → FOUND (now in a segment)

  t=30m   FLUSH happens → segment fsync'd to disk, translog cleared

Why not refresh immediately?
  Creating a segment involves:
    1. Building the inverted index for the buffered docs
    2. Building doc values
    3. Writing to filesystem cache
    4. Opening a new searcher that includes the new segment

  If you did this for EVERY document, you'd spend all your time
  building segments instead of indexing. Batching into 1-second
  intervals amortizes the cost.

You can force a refresh:
  POST /products/_refresh    ← makes all buffered docs searchable NOW
  But doing this per-document defeats the purpose.

For bulk indexing (like reindexing millions of docs):
  Set refresh_interval to "30s" or "-1" (disabled) during bulk load.
  Then re-enable when done. Massive throughput improvement.
```

---

## 10. Mapping — The Schema

### 10.1 Dynamic vs. Explicit Mapping

```
Dynamic mapping (default): ES guesses the type from the first document.

  Document: { "title": "shoes", "price": 59.99, "in_stock": true }

  ES infers:
    title    → text (with keyword sub-field)
    price    → float
    in_stock → boolean

  THE PROBLEM: If a later document has "price": "59.99" (string),
  it will FAIL because the mapping says float. And you can't change
  a field's type — you must reindex.

  Also: if someone indexes { "metadata": { "user_123": { "a": 1 } } },
  ES creates a mapping for every unique key → "mapping explosion" →
  cluster crashes from too many fields.

Explicit mapping (production): Define types upfront.

  PUT /products
  {
    "settings": {
      "number_of_shards": 3,
      "number_of_replicas": 1
    },
    "mappings": {
      "dynamic": "strict",           ← reject unmapped fields
      "properties": {
        "title": {
          "type": "text",
          "analyzer": "english",
          "fields": {
            "keyword": { "type": "keyword" }
          }
        },
        "price":      { "type": "scaled_float", "scaling_factor": 100 },
        "brand":      { "type": "keyword" },
        "created_at": { "type": "date" },
        "location":   { "type": "geo_point" },
        "tags":       { "type": "keyword" }
      }
    }
  }
```

### 10.2 Common Field Types

```
┌──────────────┬─────────────────────────────────────────────────────┐
│ Type         │ What / When                                         │
├──────────────┼─────────────────────────────────────────────────────┤
│ text         │ Full-text search. Analyzed. Can't sort/aggregate.   │
│ keyword      │ Exact values. Enums, IDs, tags. Sort/agg OK.       │
│ long/integer │ Numeric. Range queries, sorting.                    │
│ float/double │ Decimal numbers.                                    │
│ scaled_float │ Float stored as long (price × 100). More compact.  │
│ date         │ Dates. Stored as epoch millis internally.           │
│ boolean      │ true/false.                                         │
│ geo_point    │ Lat/lon. Radius queries, distance sorting.          │
│ nested       │ Array of objects with independent field matching.   │
│              │ (Default "object" flattens → cross-matching bugs.)  │
│ join         │ Parent-child relationships. Expensive. Avoid.       │
└──────────────┴─────────────────────────────────────────────────────┘
```

---

## 11. Common Query Patterns

### 11.1 Full-Text Search

```json
// "match" → analyzes the query, OR of terms by default
{ "query": { "match": { "title": "running shoes" } } }
// Finds docs with "running" OR "shoes" (or stemmed variants)

// "match_phrase" → terms must appear in exact order, adjacent
{ "query": { "match_phrase": { "title": "running shoes" } } }
// Only finds "running shoes" as a phrase, not "shoes for running"

// "multi_match" → search across multiple fields
{ "query": { "multi_match": {
    "query": "running shoes",
    "fields": ["title^3", "description"]
}}}
// title matches are 3× more important than description matches
```

### 11.2 Compound Queries

```json
{
  "query": {
    "bool": {
      "must":     [ { "match": { "title": "running shoes" } } ],
      "should":   [ { "match": { "brand": "nike" } } ],
      "filter":   [ { "range": { "price": { "gte": 50, "lte": 150 } } } ],
      "must_not": [ { "term":  { "status": "discontinued" } } ]
    }
  }
}

// must:     MUST match, affects score
// should:   OPTIONAL, boosts score if matched
// filter:   MUST match, does NOT affect score (cached!)
// must_not: MUST NOT match, does NOT affect score
```

### 11.3 Autocomplete (Search-as-You-Type)

```
How to build autocomplete:

  Option 1: Edge N-Gram Tokenizer (fastest)

    Index "elasticsearch" → ["e", "el", "ela", "elas", "elast", ...]

    User types "elas" → instant match.

    Setting:
      "tokenizer": {
        "type": "edge_ngram",
        "min_gram": 2,
        "max_gram": 15
      }

  Option 2: Completion Suggester (built-in, uses FST in memory)

    Special "completion" field type. Stored entirely in memory.
    Fastest for prefix completion but less flexible.

  Option 3: search_as_you_type field type (ES 7.2+)

    Built-in field that combines edge n-grams with shingle tokens.
    Handles both prefix matching and infix matching.
```

---

## 12. Scaling and Operational Concerns

### 12.1 Shard Sizing

```
                        ┌───────────────────────────────────────────┐
                        │ Shard sizing rules of thumb:              │
                        │                                           │
                        │ • Target: 10–50 GB per shard              │
                        │ • Max: ~65 GB (heap pressure beyond this) │
                        │ • Min docs per shard: ~100K               │
                        │ • Max shards per node: ~20 per GB of heap │
                        │                                           │
                        │ Too many small shards:                    │
                        │   Each shard = Lucene index = overhead    │
                        │   (file handles, memory, threads)         │
                        │   1000 tiny shards >> 10 right-sized ones │
                        │                                           │
                        │ Too few large shards:                     │
                        │   Can't parallelize across nodes          │
                        │   Rebalancing moves huge chunks of data   │
                        │   Recovery takes forever                  │
                        └───────────────────────────────────────────┘

Example sizing:
  500 GB of data, 3 data nodes
  → 500 GB / 30 GB per shard ≈ 17 primary shards
  → With 1 replica: 34 total shards / 3 nodes ≈ 11 shards per node ✓
```

### 12.2 Index Aliases — Zero-Downtime Reindexing

```
You can't change field types or shard count on an existing index.
Solution: reindex into a new index, swap an alias.

  Step 1: Application always queries alias "products" (not "products_v1")

    products (alias) → products_v1 (actual index)

  Step 2: Create new index with updated mapping

    PUT /products_v2 { ... new settings/mapping ... }

  Step 3: Reindex data

    POST /_reindex { "source": { "index": "products_v1" },
                     "dest":   { "index": "products_v2" } }

  Step 4: Atomic alias swap

    POST /_aliases {
      "actions": [
        { "remove": { "index": "products_v1", "alias": "products" } },
        { "add":    { "index": "products_v2", "alias": "products" } }
      ]
    }

  This is a single atomic operation. Zero downtime. Application never notices.
```

### 12.3 Index Lifecycle Management (ILM) for Time-Series Data

```
For logs, metrics, events — data has a lifecycle:

  Hot phase:   New data. Fast SSDs. Full replicas. Frequent writes.
  Warm phase:  Older data. Cheaper storage. Read-only. Maybe shrink shards.
  Cold phase:  Rarely accessed. Cheapest storage. Maybe freeze index.
  Delete:      Data retention expired. Auto-delete.

  logs-2024-01 → [HOT for 7 days] → [WARM for 30 days] → [COLD for 90 days] → DELETE

ILM automates this — indexes automatically roll over and transition.
Combined with data tiers (hot/warm/cold nodes with different hardware).
```

---

## 13. ES is NOT a Primary Database

```
Why you MUST keep a source of truth outside Elasticsearch:

  1. No transactions. No ACID. Index + replica is "eventually consistent."
     Two near-simultaneous writes to the same doc? Last-write-wins.

  2. Index corruption happens. Lucene segments can get corrupted.
     Recovery from replica helps, but replicas can diverge.

  3. Reindexing is a fact of life. You WILL need to change mappings.
     If ES is your only copy, reindexing means downtime.

  4. No referential integrity. No foreign keys, no joins worth using.
     The "join" field type exists but is extremely limited.

The correct architecture:

  ┌────────────┐     ┌──────────────┐     ┌──────────────┐
  │  Postgres  │────→│  Change Data │────→│ Elasticsearch │
  │  (source   │     │  Capture CDC │     │  (search     │
  │   of truth)│     │  or queue    │     │   index)     │
  └────────────┘     └──────────────┘     └──────────────┘

  Writes go to Postgres. CDC (Debezium/Kafka) streams changes to ES.
  ES is a read-optimized search index, not the primary store.
  If ES corrupts, rebuild it from Postgres.
```

---

## 14. Key Numbers

```
Refresh interval:          1 second (default, configurable)
Flush interval:            ~30 minutes or when translog hits 512MB
Bulk request size:         5–15 MB sweet spot
Shard target size:         10–50 GB
Max heap:                  50% of RAM, capped at 32 GB (compressed oops)
Remaining RAM:             For filesystem cache (segments are memory-mapped)
Recovery speed:            ~40 MB/s per shard (default throttle)
Max result window:         10,000 (from + size ≤ 10000; use search_after beyond)
Master election timeout:   ~30 seconds
Cluster state max fields:  ~1000 per index (before mapping explosion risk)
```

---

## 15. Common Pitfalls → What to Do Instead

```
❌ Using ES as primary database
✅ Source of truth in Postgres/MySQL, CDC → ES for search

❌ Dynamic mapping in production
✅ "dynamic": "strict", explicit mapping for every field

❌ Too many small shards (over-sharding)
✅ Target 10–50 GB per shard, shrink old indexes

❌ Not using filters for exact-match conditions
✅ Put exact matches in "filter" context → cached, no scoring overhead

❌ Deep pagination with from/size (from: 100000)
✅ Use search_after for deep pagination, or scroll API for exports

❌ Not planning for reindexing
✅ Use aliases from day 1, always query the alias

❌ Mapping explosion from user-controlled fields
✅ Flatten user data or use "flattened" field type

❌ Searching on keyword fields / aggregating on text fields
✅ Use text for search bodies, keyword for exact match/sort/agg
```
