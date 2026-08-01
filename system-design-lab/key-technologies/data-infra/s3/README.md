# S3 — How Object Storage Was Designed

---

## 1. The Problem S3 Solves

```
Before S3 (pre-2006), if you needed to store files:

  Option 1: Buy disk arrays, set up NFS/CIFS, manage RAID, handle failures.
  Option 2: Put files in a database BLOB column (terrible for large files).
  Option 3: FTP servers. Good luck scaling that.

  Problems with all of these:
    - YOU manage hardware, capacity planning, backups, replication
    - Limited to one datacenter (fire/flood = total loss)
    - Scaling from 1 TB to 1 PB requires redesigning everything
    - No HTTP access — can't serve files directly to the web

  S3 (launched March 2006): "store any amount of data, pay per GB, access via HTTP."
    - 11 nines of durability (99.999999999%) — 10M objects → lose 1 every 10,000 years
    - Infinite scale (S3 stores over 350 trillion objects as of 2024)
    - Simple API: PUT, GET, DELETE, LIST
    - No filesystem — flat namespace of buckets + keys

  S3 kicked off the cloud era. It's the most-used AWS service by far.
```

---

## 2. Data Model — It's NOT a Filesystem

```
S3 looks like a filesystem but it is NOT one. There are no directories.

  Bucket:  "my-app-images"
  Key:     "users/alice/profile.jpg"
  Object:  [binary data + metadata]

  "users/alice/profile.jpg" is a FLAT STRING KEY.
  The "/" has no special meaning to S3. It's just a character.
  The S3 console shows "folders" by splitting on "/" — that's a UI trick.

  Why this matters:
    - "Rename" a file = COPY to new key + DELETE old key (no atomic rename)
    - "List all files in users/alice/" = prefix scan with delimiter "/"
    - No "move" operation. No hard links. No symlinks.
    - Every object is independent — no parent directory to create first.

  ┌──────────────────────────────────────────────────────────────┐
  │ Bucket: my-app                                               │
  │                                                              │
  │ Key                          Size     Storage Class          │
  │ ────────────────────────     ──────   ──────────────         │
  │ users/alice/profile.jpg      245 KB   STANDARD               │
  │ users/alice/photo-01.jpg     1.2 MB   STANDARD               │
  │ users/bob/profile.jpg        189 KB   STANDARD               │
  │ logs/2024/01/access.gz       50 MB    GLACIER                │
  │ models/v3/weights.bin        14 GB    INTELLIGENT_TIERING    │
  │                                                              │
  │ These are NOT in folders. They're flat key-value pairs.      │
  │ The index is a sorted key → metadata mapping.                │
  └──────────────────────────────────────────────────────────────┘
```

### 2.1 Namespace vs. Ownership — Why Two AWS Customers Do Not Collide

```
An object's identity is:

  (AWS partition, bucket name, object key [, version ID])

For ordinary S3 general-purpose buckets, the bucket name is allocated to
exactly one owner within an AWS partition (such as `aws`, `aws-cn`, or
`aws-us-gov`). Object keys only need to be unique INSIDE that bucket.

Example:

  Alice creates bucket "alice-data":
    CreateBucket("alice-data") → success; Alice owns the bucket.

  Bob tries to create the same bucket name:
    CreateBucket("alice-data") → BucketAlreadyExists

  Bob then tries to write to Alice's existing bucket:
    PUT s3://alice-data/report.csv, signed with Bob's credentials
    → this names ALICE'S bucket, not a private Bob namespace
    → S3 authenticates Bob, evaluates Alice's bucket policy + IAM
    → 403 AccessDenied unless Alice explicitly granted Bob access

  Bob creates his own differently named bucket:
    s3://bob-data/report.csv

  Alice and Bob can both use the key "report.csv" safely because:
    (alice-data, report.csv) != (bob-data, report.csv)

  ┌──────────────────────────────────────────────────────────────┐
  │ Bucket name selects the OWNER'S namespace.                   │
  │ Object key selects an object WITHIN that namespace.          │
  │ Caller identity does not silently create another namespace.  │
  │ A request is authorized against the selected bucket.         │
  └──────────────────────────────────────────────────────────────┘

The caller does NOT need to discover everyone else's object keys. A key in
another bucket cannot collide with theirs. Knowing or guessing another
customer's bucket/key also grants no access; S3 authorizes every request.

How can a finite 63-character bucket namespace be large enough?

  Bucket names are identifiers, not English words. A general-purpose bucket
  name can contain up to 63 lowercase letters, digits, hyphens, and periods.
  Even using ONLY the 36 letters/digits gives roughly:

    36^63 ≈ 10^98 possible 63-character names

  One trillion is only 10^12. More importantly, S3 stores trillions of
  OBJECTS, not trillions of BUCKETS. A single bucket can contain enormous
  numbers of object keys, and each key may be up to 1,024 UTF-8 bytes.

  Applications commonly append a UUID:
    company-prod-data-a3f2b1c45d6e7f8091a2b3c4d5e6f789

  A generated name is not assumed unique. CreateBucket performs an exact
  uniqueness check:
    name available → reserve it for this owner
    name already reserved → BucketAlreadyExists; generate another and retry

  Therefore a naming COLLISION ATTEMPT is possible, but two owners cannot
  simultaneously hold the same shared-global bucket name. This is the same
  distinction as a database PRIMARY KEY: callers may submit the same value,
  but the uniqueness constraint prevents two stored rows with that key.

  AWS also offers an account-regional bucket namespace whose generated suffix
  includes the AWS account ID and Region. Those names are reserved to that
  account and cannot later be recreated by another account.

Hashes do not define identity:

  Conceptually, S3 may hash or partition names for routing, but object identity
  is the FULL exact (bucket name, object key) byte sequence. If an internal
  hash maps two different names to the same partition, the index still compares
  their complete strings, just as a normal hash table handles hash collisions.
  AWS does not publish the exact current partition-routing algorithm.

The same-key overwrite case exists only when two principals are BOTH allowed
to write to the SAME bucket and choose the SAME key:

  Service A (authorized) → PUT s3://shared-bucket/report.csv
  Service B (authorized) → PUT s3://shared-bucket/report.csv

S3 does not automatically add the principal ID to the key. Without bucket
versioning, one write becomes the current object; with versioning, both writes
are retained under different version IDs and one is current. Applications that
share a bucket must therefore enforce prefixes, unique IDs, conditional writes,
or a table/catalog commit protocol.
```

---

## 3. Internal Architecture — How S3 Actually Works

AWS has never published a full S3 architecture paper, but enough has been
disclosed at re:Invent talks and through the Dynamo paper (S3's metadata layer
is inspired by it) to understand the design.

### 3.1 The Three Layers

```
S3 is built from three separate distributed systems:

  ┌─────────────────────────────────────────────────────────────────┐
  │                                                                 │
  │  Layer 1: FRONT-END (Request Routing)                          │
  │  ┌───────────────────────────────────────────────────────────┐ │
  │  │  Stateless HTTP fleet. Parses requests, authenticates,    │ │
  │  │  authorizes (IAM policies), routes to the right partition.│ │
  │  │  This layer handles billions of requests/day.             │ │
  │  └───────────────────────────────────────────────────────────┘ │
  │       │                                                        │
  │       ▼                                                        │
  │  Layer 2: METADATA (Index / Namespace)                         │
  │  ┌───────────────────────────────────────────────────────────┐ │
  │  │  Distributed key-value store that maps:                   │ │
  │  │    (bucket, key) → object metadata                        │ │
  │  │                                                            │ │
  │  │  Metadata includes:                                        │ │
  │  │    - Object size, ETag, content-type                      │ │
  │  │    - Storage class                                        │ │
  │  │    - WHICH data nodes hold the actual bytes               │ │
  │  │    - Encryption key reference                             │ │
  │  │    - Version ID (if versioning enabled)                   │ │
  │  │                                                            │ │
  │  │  This is a partitioned metadata store. AWS does not       │ │
  │  │  publish the exact current routing/partition algorithm.   │ │
  │  │  The full (bucket, key) bytes remain the object identity; │ │
  │  │  a routing hash, if used, is not treated as unique.       │ │
  │  │  Replicated across multiple AZs.                          │ │
  │  │                                                            │ │
  │  │  This is the hardest part of S3 to build at scale.        │ │
  │  │  It must handle LIST operations (prefix scans) across     │ │
  │  │  trillions of keys while remaining consistent.            │ │
  │  └───────────────────────────────────────────────────────────┘ │
  │       │                                                        │
  │       ▼                                                        │
  │  Layer 3: DATA (Object Storage)                                │
  │  ┌───────────────────────────────────────────────────────────┐ │
  │  │  The actual bytes. Stored on spinning disks (vast and     │ │
  │  │  cheap). Objects are split into chunks, erasure-coded,    │ │
  │  │  and spread across multiple disks in multiple AZs.        │ │
  │  │                                                            │ │
  │  │  Each chunk has a placement group → physical disk mapping.│ │
  │  │  A background process continuously verifies integrity     │ │
  │  │  (checksums) and re-replicates if disks fail.             │ │
  │  └───────────────────────────────────────────────────────────┘ │
  │                                                                 │
  └─────────────────────────────────────────────────────────────────┘
```

### 3.2 How a PUT (Upload) Works

```
Client: PUT /my-bucket/photos/cat.jpg (5 MB)
  │
  ▼
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│ Step 1: FRONT-END receives the HTTP request                         │
│   - Parse bucket name + key                                         │
│   - Authenticate (AWS Signature V4 — HMAC of request)               │
│   - Check IAM policy: does this identity have s3:PutObject?         │
│   - Check bucket policy, ACLs                                       │
│                                                                      │
│ Step 2: FRONT-END calls METADATA layer                              │
│   - "I need to create/overwrite object (my-bucket, photos/cat.jpg)" │
│   - Metadata layer assigns placement: which data nodes will store   │
│     the chunks. Picks nodes spread across ≥3 AZs.                  │
│                                                                      │
│ Step 3: FRONT-END streams bytes to DATA layer                       │
│   - The 5 MB is split into chunks (say, 4 MB + 1 MB)              │
│   - Each chunk is erasure-coded:                                    │
│     Original chunk → N data shards + M parity shards               │
│     (e.g., 6+3 = 9 total. Can lose ANY 3 and reconstruct.)        │
│   - Shards are written to different disks across ≥3 AZs            │
│                                                                      │
│ Step 4: DATA layer confirms all shards are written                  │
│   - Each shard is checksummed (CRC or SHA-256)                      │
│   - Shard locations are returned to the metadata layer              │
│                                                                      │
│ Step 5: METADATA layer commits the index entry                      │
│   - (bucket, key) → { size: 5MB, etag: "abc123",                   │
│                        shards: [node1:disk5, node2:disk3, ...],     │
│                        created: 2024-01-15T10:30:00Z }              │
│   - This is the point-of-no-return. After this, the object exists. │
│                                                                      │
│ Step 6: Return 200 OK + ETag to client                              │
│                                                                      │
│ Total: data written across 3+ AZs, erasure-coded, checksummed,     │
│ metadata indexed. Durability: 99.999999999% from this moment.       │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.3 How a GET (Download) Works

```
Client: GET /my-bucket/photos/cat.jpg
  │
  ▼
┌──────────────────────────────────────────────────────────────────────┐
│                                                                      │
│ Step 1: FRONT-END authenticates + authorizes (same as PUT)          │
│                                                                      │
│ Step 2: FRONT-END queries METADATA layer                            │
│   - Look up (my-bucket, photos/cat.jpg)                             │
│   - Get back: object size, shard locations, checksum                │
│                                                                      │
│ Step 3: FRONT-END reads from DATA layer                             │
│   - Contact the data nodes holding the shards                       │
│   - Read enough shards to reconstruct (only need N of N+M)         │
│   - If some shards are slow or unavailable, read from others        │
│     (erasure coding gives you automatic redundancy + speed)         │
│   - Verify checksums on each shard                                  │
│   - Decode erasure coding → reconstruct original bytes              │
│                                                                      │
│ Step 4: Stream bytes to client                                      │
│   - HTTP response with Content-Length, ETag, etc.                   │
│   - For large objects: data streams as it's reassembled             │
│                                                                      │
│ Latency: first byte in ~50-100ms (STANDARD class)                  │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.4 Erasure Coding — How 11 Nines of Durability Works

```
Why not just replicate 3×?

  3× replication: store 1 GB → costs 3 GB of disk
  Durability: if disk failure rate is 1%/year and you replicate 3×
  across 3 AZs, you'd need all 3 copies to fail simultaneously.
  That's ~99.9999% durability. Good. But S3 claims 99.999999999%.
  And 3× storage cost is expensive at exabyte scale.

Erasure coding: store 1 GB → costs ~1.5 GB of disk (50% overhead, not 200%)
  AND you get HIGHER durability.

How it works (simplified):

  Original data: [A] [B] [C] [D]  (4 data chunks)

  Reed-Solomon encoding generates parity chunks:
  [P1] = f(A, B, C, D)
  [P2] = g(A, B, C, D)

  Store 6 chunks on 6 different disks:
  [A] [B] [C] [D] [P1] [P2]

  ANY 4 of the 6 chunks → can reconstruct ALL original data.
  You can lose ANY 2 disks and recover completely.

  ┌──────────────────────────────────────────────────────────────┐
  │  AZ-1           AZ-2           AZ-3                          │
  │  ┌────┐        ┌────┐         ┌────┐                        │
  │  │ A  │        │ C  │         │ P1 │                        │
  │  │ B  │        │ D  │         │ P2 │                        │
  │  └────┘        └────┘         └────┘                        │
  │                                                              │
  │  If AZ-3 burns down: A, B, C, D still available → full data │
  │  If AZ-1 + one disk in AZ-2 fails:                         │
  │    Have: C, D, P1, P2 → reconstruct A and B. No data loss. │
  └──────────────────────────────────────────────────────────────┘

  S3 uses something like 6+3 (need any 6 of 9) or similar ratios.
  Combined with continuous integrity checking and automatic repair,
  this is how you get 11 nines.

  Storage overhead comparison:
    3× replication:     3.0× raw storage → durability ~6 nines
    Erasure coding 6+3: 1.5× raw storage → durability ~11 nines
    Erasure coding wins on BOTH cost and durability.
```

### 3.5 Integrity Checking — How S3 Catches Corruption

```
Disks lie. A write() syscall returning success does NOT guarantee the bytes
actually reached the platter correctly. S3 defends against this at every layer.

ON WRITE (inline verification):

  Step 3 writes 9 shards to 9 disks. Then for EACH shard:

    1. Data node computes: checksum = CRC32C(shard_bytes)
       e.g., CRC32C(0x4A 7B 3F ...) = 0xA1B2C3D4

    2. Checksum is stored in shard metadata (not inside the shard bytes):
       { shard_id: "abc-007", disk: "az1-rack3-disk5",
         size: 894231, checksum: "0xA1B2C3D4" }

    3. Data node reads the shard BACK from disk, recomputes checksum,
       and compares. Match → write actually landed correctly.
       (Catches: torn writes, firmware bugs, bad sectors)

    4. Only THEN does the data node respond: "shard abc-007 confirmed."

ON READ (inline verification):

  Every GET recomputes and verifies the checksum on each shard.
  Mismatch → that shard is silently discarded and another shard is
  used instead (erasure coding provides redundant shards).
  The corrupted shard is flagged for repair.

BACKGROUND SCRUBBING (continuous, 24/7):

  This runs on every disk in every data center, all the time:

    for each shard on this disk:
        read shard from disk
        compute checksum
        compare to stored checksum

        if mismatch:
            mark shard as CORRUPT
            trigger re-replication:
                1. Read enough good shards from other disks
                2. Reconstruct original data (erasure decoding)
                3. Re-encode a new replacement shard
                4. Write it to a DIFFERENT healthy disk
                5. Update metadata with new shard location

    Cycle time: entire disk scrubbed every ~few days
    (rate-limited to avoid impacting read/write throughput)

WHY THIS GIVES 11 NINES:

  Data loss requires MORE shards to fail than erasure coding can
  tolerate, BEFORE the scrubber detects and repairs them.

  With 6+3 coding, you need 4+ shard failures before repair:

    Disk failure rate:     ~1-2% per year
    Scrub cycle:           every few days
    Repair time:           minutes (reconstruct + write new shard)

    Probability that 4 of 9 shards fail within the same repair
    window across 3 different AZs = astronomically small → 11 nines

  BIT ROT is the real enemy, not disk death:
    - A dead disk is obvious — you know immediately
    - Bit rot (silent data corruption) is sneaky — a bit flips and
      nobody notices until someone reads it months later
    - Background scrubbing catches bit rot EARLY, before the
      corruption window overlaps with other failures
    - This is the same principle as ZFS scrubbing or HDFS block scanner

WHY O_DIRECT + FSYNC IS REQUIRED FOR CHECKSUMS TO BE REAL:

  Without O_DIRECT, the read-back checksum is theater:

    WITHOUT O_DIRECT:
      write("hello")  → goes to OS page cache (RAM)
      read()          → returns from page cache (RAM)
      checksum match? → YES, but you verified nothing about the disk!

      Page cache              Disk platter
      ┌─────────┐            ┌─────────┐
      │ "hello" │            │ garbage  │  ← could be anything
      └─────────┘            └─────────┘
           ↑ read
           ↑ (never touched disk)

    WITH O_DIRECT + fsync:
      write("hello")  → goes directly to disk (bypasses cache)
      fsync()         → disk controller flushes to platter
      read()          → reads from actual platter (bypasses cache)
      checksum match? → YES, and you KNOW the platter has the right data

      Page cache              Disk platter
      ┌─────────┐            ┌─────────┐
      │ (empty) │            │ "hello" │  ← verified!
      └─────────┘            └─────────┘
                                  ↑ read from here

  S3 data nodes almost certainly use:
    - O_DIRECT for all shard I/O (read AND write) — bypasses page cache
      (they don't want page cache anyway: objects are large, accessed once,
       and would pollute the cache for other operations)
    - fsync after writes — ensures disk controller commits to platter
    - Possibly hardware-level checksums (T10-DIF / T10-PI) where the
      disk controller itself verifies, but this varies by drive vendor

  Critical insight: fsync alone is NOT enough.
    Even after fsync, a read() without O_DIRECT might still return from
    the page cache. You need O_DIRECT on the READ to guarantee you're
    hitting the actual platter.
```

### 3.6 Strong Consistency (Since December 2020)

```
Before 2020, S3 was eventually consistent for overwrites and deletes:

  PUT photos/cat.jpg (version 1)
  PUT photos/cat.jpg (version 2)    ← overwrite
  GET photos/cat.jpg                ← might return version 1!

  This was S3's most infamous quirk. It bit everyone.

Since December 2020, S3 is STRONGLY CONSISTENT:

  PUT photos/cat.jpg (version 2)
  GET photos/cat.jpg                ← always returns version 2

  How they achieved this (re:Invent 2021 talk):

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  They added "witness" nodes to the metadata layer.          │
  │                                                              │
  │  Before a PUT returns 200:                                  │
  │    1. Data is stored (erasure-coded, multi-AZ)              │
  │    2. Metadata is committed with a monotonic "logical clock"│
  │    3. ALL metadata replicas have the new version             │
  │                                                              │
  │  On GET:                                                     │
  │    1. Metadata read checks the logical clock                │
  │    2. If the local replica is stale, it fetches from leader │
  │    3. Always returns the latest committed version            │
  │                                                              │
  │  S3 achieved this with NO performance penalty.              │
  │  The trick: the metadata layer was already doing multi-AZ   │
  │  writes for PUT; they just made GET wait for consistency    │
  │  instead of reading from any replica.                       │
  │                                                              │
  │  This is read-after-write consistency (strongest form):     │
  │    After PUT succeeds, any subsequent GET sees it.          │
  │    After DELETE succeeds, any subsequent GET returns 404.   │
  │    LIST sees the object immediately after PUT.              │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

---

## 4. Multipart Upload — How Large Files Work

```
Problem: uploading a 100 GB file in one HTTP request?
  - Any network hiccup = restart from scratch
  - Can't parallelize (one TCP stream)
  - HTTP timeouts

Solution: Multipart upload.

  Step 1: Initiate multipart upload → get an Upload ID
  Step 2: Upload parts in parallel (5 MB - 5 GB each)
  Step 3: Complete multipart upload → S3 assembles the object

  ┌──────────────────────────────────────────────────────┐
  │                                                      │
  │  100 GB file                                         │
  │    │                                                 │
  │    ├── Part 1 (100 MB) ──→ S3 ──→ ETag 1           │
  │    ├── Part 2 (100 MB) ──→ S3 ──→ ETag 2    (parallel!)
  │    ├── Part 3 (100 MB) ──→ S3 ──→ ETag 3           │
  │    ├── ...                                           │
  │    └── Part 1000 (100 MB) ──→ S3 ──→ ETag 1000     │
  │                                                      │
  │  Complete: "here are all 1000 ETags, assemble them." │
  │  S3 concatenates → single object accessible at key.  │
  │                                                      │
  │  If Part 7 fails: retry ONLY Part 7. Not the whole  │
  │  100 GB.                                             │
  │                                                      │
  │  Max parts: 10,000                                   │
  │  Max object size: 5 TB                               │
  │  Min part size: 5 MB (except last part)              │
  └──────────────────────────────────────────────────────┘

  Abandoned multipart uploads cost money (partial parts on disk).
  Always set a lifecycle rule to auto-abort incomplete uploads.
```

---

## 5. Storage Classes — The Economics of Cold Data

```
Not all data is accessed equally. S3 offers storage classes that trade
retrieval speed for lower storage cost:

  ┌──────────────────┬──────────┬───────────┬──────────────────────┐
  │ Storage Class     │ $/GB/mo  │ Retrieval │ Use case             │
  ├──────────────────┼──────────┼───────────┼──────────────────────┤
  │ STANDARD          │ $0.023   │ Instant   │ Frequently accessed  │
  │ INTELLIGENT_TIER  │ $0.023*  │ Instant   │ Unknown access       │
  │                   │          │           │ pattern (auto-moves) │
  │ STANDARD_IA       │ $0.0125  │ Instant   │ Once/month access    │
  │ ONE_ZONE_IA       │ $0.010   │ Instant   │ Re-creatable data    │
  │ GLACIER_IR        │ $0.004   │ Instant   │ Quarterly archives   │
  │ GLACIER_FLEX      │ $0.0036  │ 1-12 hrs  │ Yearly archives      │
  │ GLACIER_DEEP      │ $0.00099 │ 12-48 hrs │ Compliance vaults    │
  └──────────────────┴──────────┴───────────┴──────────────────────┘

  STANDARD vs GLACIER_DEEP: 23× cheaper storage. Tradeoff: 48hr retrieval.

How Glacier works internally:
  Data is written to tape archives (yes, actual magnetic tapes).
  Tapes are stored in robotic tape libraries across multiple facilities.
  "Retrieval" = robot finds the tape, loads it, reads the data, copies
  to disk, makes it available via HTTP.

  This is why retrieval takes hours — it's literal physical tape handling.

Lifecycle rules automate transitions:
  Day 0:   → STANDARD (hot data, just uploaded)
  Day 30:  → STANDARD_IA (not accessed in 30 days, cheaper storage)
  Day 90:  → GLACIER_FLEX (not accessed in 90 days, way cheaper)
  Day 365: → GLACIER_DEEP (compliance retention, cheapest)
  Day 730: → DELETE (retention period expired)
```

---

## 6. Versioning — How S3 Handles Overwrites

```
With versioning enabled, S3 NEVER deletes or overwrites data.
Every write creates a new version:

  PUT cat.jpg (v1)  → version-id: "abc111"
  PUT cat.jpg (v2)  → version-id: "abc222"
  PUT cat.jpg (v3)  → version-id: "abc333"

  GET cat.jpg       → returns v3 (latest)
  GET cat.jpg?versionId=abc111 → returns v1

  DELETE cat.jpg    → adds a "delete marker" (v4)
  GET cat.jpg       → 404 (delete marker hides it)
  GET cat.jpg?versionId=abc222 → still returns v2!

  DELETE cat.jpg?versionId=abc333 → permanently removes ONLY v3

  MFA Delete: require MFA token to permanently delete a version.
  This is how you make data tamper-proof (compliance, audit trails).
```

---

## 7. Performance Design

```
S3 can handle:
  - 5,500 GET requests/sec per PREFIX
  - 3,500 PUT requests/sec per PREFIX

  A "prefix" is the key path up to the last "/":
    "images/cats/001.jpg"  → prefix = "images/cats/"
    "images/dogs/001.jpg"  → prefix = "images/dogs/"

  These are DIFFERENT prefixes → each gets its own 5,500/3,500 limit.
  Spread objects across many prefixes → effectively unlimited throughput.

  ┌──────────────────────────────────────────────────────────┐
  │  BAD: all objects under one prefix                       │
  │    data/file-001.jpg                                     │
  │    data/file-002.jpg   → all share one 5,500 GET/s limit│
  │    data/file-003.jpg                                     │
  │                                                          │
  │  GOOD: hash-based prefix distribution                    │
  │    a3f2/file-001.jpg                                     │
  │    7b1c/file-002.jpg   → each prefix = separate limit   │
  │    e9d4/file-003.jpg     → effectively unlimited         │
  │                                                          │
  │  (S3 internally partitions by key prefix. More prefixes  │
  │   = more partitions = more parallelism.)                 │
  └──────────────────────────────────────────────────────────┘

  Transfer Acceleration: upload via CloudFront edge locations
    → data enters AWS backbone at the nearest edge
    → faster for cross-continent uploads (edge → backbone vs public internet)
```

---

## 8. Security Model

```
S3 has FOUR layers of access control (and they all interact):

  1. IAM Policies (identity-based)
     "User alice can s3:GetObject on arn:aws:s3:::my-bucket/*"
     Attached to users, roles, groups.

  2. Bucket Policies (resource-based)
     "Anyone from account 123456 can read this bucket."
     Attached to the bucket itself.

  3. ACLs (legacy, avoid)
     Per-object permissions. AWS recommends disabling.

  4. Block Public Access (guardrail)
     Account-level or bucket-level setting that BLOCKS all public
     access regardless of what policies say.
     Turn this ON for every bucket unless you explicitly need public.

  Request evaluation:
    Start with DENY.
    Check IAM policy → explicit deny? → DENY
    Check bucket policy → explicit deny? → DENY
    Check block public access → would make it public? → DENY
    Check IAM policy → explicit allow?
    Check bucket policy → explicit allow?
    Both allow? → ALLOW

  Encryption:
    SSE-S3:   S3 manages keys (default since Jan 2023, automatic)
    SSE-KMS:  AWS KMS manages keys (audit trail, rotation, access control)
    SSE-C:    You provide the key with each request (S3 never stores it)
    Client-side: Encrypt before upload. S3 sees only ciphertext.
```

---

## 9. S3 vs. Filesystem vs. Block Storage vs. Database

```
┌──────────────────┬──────────────┬───────────────┬───────────────┐
│                  │   S3         │   EBS         │   EFS/NFS     │
│                  │ (object)     │ (block)       │ (filesystem)  │
├──────────────────┼──────────────┼───────────────┼───────────────┤
│ Access pattern   │ HTTP API     │ Mount as disk │ Mount as dir  │
│ Latency          │ 50-100ms     │ <1ms          │ ~2-5ms        │
│ Throughput       │ Very high    │ High          │ Medium        │
│ Max size         │ 5 TB/object  │ 64 TB/volume  │ Petabytes     │
│ Concurrent       │ Millions     │ One EC2       │ Thousands     │
│ access           │ of readers   │ instance      │ of instances  │
│ Modify in-place  │ No (replace  │ Yes           │ Yes           │
│                  │ whole object)│               │               │
│ Cost/GB/mo       │ $0.023       │ $0.08-0.10    │ $0.30         │
│ Durability       │ 11 nines     │ 5 nines       │ 11 nines      │
├──────────────────┼──────────────┼───────────────┼───────────────┤
│ Use for          │ Images, logs │ Databases,    │ Shared config │
│                  │ backups, ML  │ OS volumes    │ CMS media     │
│                  │ datasets     │               │               │
└──────────────────┴──────────────┴───────────────┴───────────────┘

S3 is for data you write once and read many times.
If you need to modify bytes in the middle of a file → use block/file storage.
If you need sub-millisecond latency → use block storage or a database.
```

---

## 10. Key Numbers

```
Max object size:              5 TB
Max PUT size (single):        5 GB (use multipart above this)
Max parts per multipart:      10,000
Min part size:                5 MB
GET throughput per prefix:    5,500 req/s
PUT throughput per prefix:    3,500 req/s
First-byte latency:          50-100ms (STANDARD)
Durability:                   99.999999999% (11 nines)
Availability:                 99.99% (STANDARD), 99.5% (ONE_ZONE_IA)
Max buckets per account:      100 (soft limit, can increase)
Max object key length:        1,024 bytes
Bucket names:                 unique within an AWS partition
```
