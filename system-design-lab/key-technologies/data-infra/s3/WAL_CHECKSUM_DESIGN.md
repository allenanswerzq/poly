# WAL-Based Checksum Integrity for Slow Disk (eMMC) File Downloads

---

## 1. The Problem

```
Constraints:
  - File download application (data is always re-downloadable)
  - Data LOSS is acceptable (just re-download)
  - Data CORRUPTION is NOT acceptable (silent bit flips = poison)
  - Disk is slow eMMC (~2-5 MB/s random write, fsync = 50-200ms)
  - fsync on every file is too expensive on eMMC

The danger:
  Download a 500 MB video → write to eMMC → power loss mid-write
  → file is half-written but looks complete (OS reports full size)
  → app plays a corrupted video, user sees garbage

  Or worse: bit rot months later, file silently corrupted,
  no way to know without re-downloading and comparing.
```

---

## 2. Design Overview

```
Two components, two different durability strategies:

  ┌─────────────────────────────────────────────────────────────┐
  │                                                             │
  │  WAL (Write-Ahead Log)                                     │
  │  ┌───────────────────────────────────────────────────────┐ │
  │  │  Stores ONLY checksums (not data). Tiny writes.       │ │
  │  │  Append-only. Periodic batch fsync (every N seconds). │ │
  │  │  This is our source of truth for "is this file good?" │ │
  │  └───────────────────────────────────────────────────────┘ │
  │                                                             │
  │  Storage (data files)                                      │
  │  ┌───────────────────────────────────────────────────────┐ │
  │  │  Direct write, NO fsync per file.                     │ │
  │  │  OS flushes when it wants (writeback cache).          │ │
  │  │  May be corrupt after power loss — that's OK,         │ │
  │  │  we detect it via WAL checksums and re-download.      │ │
  │  └───────────────────────────────────────────────────────┘ │
  │                                                             │
  └─────────────────────────────────────────────────────────────┘

Key insight: WAL is TINY (just checksums), so fsync'ing WAL is cheap.
  A 500 MB file with 1 MB chunks = 500 checksum entries = ~16 KB in WAL.
  fsync'ing 16 KB on eMMC = fast. fsync'ing 500 MB = slow.
```

---

## 3. Data Flow

### 3.1 Download + Write Path

```
Download: GET https://cdn.example.com/video.mp4 (500 MB)

  ┌────────────────────────────────────────────────────────────────┐
  │                                                                │
  │  Step 1: Start download, stream in 1 MB chunks                │
  │                                                                │
  │  For each chunk:                                               │
  │    ┌─────────────────────────────────────────────────────────┐ │
  │    │                                                         │ │
  │    │  a) Receive chunk N from network into memory buffer     │ │
  │    │                                                         │ │
  │    │  b) Compute checksum in memory:                         │ │
  │    │     crc = CRC32C(chunk_bytes)                           │ │
  │    │                                                         │ │
  │    │  c) Append to WAL (in memory WAL buffer):               │ │
  │    │     { file_id, chunk_index: N, offset, size, crc }      │ │
  │    │                                                         │ │
  │    │  d) Write chunk to data file (no fsync):                │ │
  │    │     pwrite(fd, chunk_bytes, offset=N*CHUNK_SIZE)        │ │
  │    │                                                         │ │
  │    └─────────────────────────────────────────────────────────┘ │
  │                                                                │
  │  Step 2: After all chunks written:                             │
  │    - Append to WAL: { file_id, status: COMPLETE,               │
  │                        total_chunks: 500, file_crc: <whole> }  │
  │                                                                │
  │  Step 3: Commit (mirroring for app/data/envocnfig, call fsync  |
  |      first to ensure wal goes to disk)                         │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘
```

### 3.2 Verification Path (Background, Periodic)

```
A background thread runs on a CONFIGURABLE SCHEDULE (e.g., every 1 hour).
This is the corruption detection window — our guarantee is:

  "Any corruption will be detected within at most <verify_interval>."

  ┌────────────────────────────────────────────────────────────────┐
  │                                                                │
  │  Every verify_interval (default: 1 hour):                     │
  │                                                                │
  │  For each file tracked in WAL:                                │
  │                                                                │
  │    1. Read WAL entries for this file_id                       │
  │       → get list of (chunk_index, offset, size, expected_crc) │
  │       → first validate each WAL entry's own record_hash       │
  │         (if WAL entry itself is bad → file is untrusted)      │
  │                                                                │
  │    2. For each chunk:                                          │
  │       actual_bytes = pread(data_fd, size, offset)              │
  │       actual_crc = CRC32C(actual_bytes)                        │
  │                                                                │
  │       if actual_crc != expected_crc:                           │
  │         → file is CORRUPT                                     │
  │         → delete data file                                    │
  │         → schedule full re-download (no partial repair)       │
  │                                                                │
  │    3. All chunks pass?                                         │
  │       → Update WAL: { file_id, status: VERIFIED }             │
  │       → fsync WAL                                              │
  │       → File is trusted until next verify cycle               │
  │                                                                │
  │  This catches:                                                │
  │    - Power loss corruption (data not fsync'd to platter)      │
  │    - Bit rot (silent corruption over time)                    │
  │    - Firmware bugs                                            │
  │                                                                │
  │  Configurable:                                                │
  │    verify_interval: Duration  (default 1h, trade freshness    │
  │                                vs I/O load on eMMC)           │
  │                                                                │
  └────────────────────────────────────────────────────────────────┘
```

### 3.3 Recovery After Crash

```
On app startup:

  1. Open WAL file, validate header hash
  2. Read all entries, validating each record_hash
     (discard any entry with bad record_hash — WAL corruption)
  3. Build in-memory state:

     file_id=abc → COMPLETE (all checksums recorded, not yet verified)
     file_id=def → PARTIAL  (only 300 of 500 chunks recorded)
     file_id=ghi → VERIFIED (previously confirmed good)

  4. For COMPLETE files → schedule verification (3.2)
  5. For PARTIAL files → discard data file, re-download from scratch
     (WAL fsync hadn't happened, checksums unreliable)
  6. For VERIFIED files → trusted, will be re-checked on next verify cycle
  7. For files with corrupt WAL entries → discard data, re-download
     (can't trust what we can't verify)
```

---

## 4. WAL Format

```
WAL is a simple append-only binary file.

File: /data/downloads/wal/checksum.wal

Header (64 bytes):
  ┌──────────┬──────────┬──────────┬──────────┬──────────────┐
  │ magic    │ version  │ seq_num  │ hdr_hash │ reserved     │
  │ 4 bytes  │ 4 bytes  │ 8 bytes  │ 4 bytes  │ 44 bytes     │
  │ "CWAL"   │ 1        │ monotonic│ CRC32C   │              │
  │          │          │          │ (of hdr) │              │
  └──────────┴──────────┴──────────┴──────────┴──────────────┘
  hdr_hash = CRC32C(magic + version + seq_num + reserved)
  On open: if hdr_hash doesn't match → WAL is corrupt → rebuild.

Chunk Entry (36 bytes each):
  ┌──────────┬────────────┬──────────┬──────────┬──────────┬─────────────┐
  │ file_id  │ chunk_idx  │ offset   │ size     │ crc32c   │ record_hash │
  │ 8 bytes  │ 4 bytes    │ 8 bytes  │ 4 bytes  │ 4 bytes  │ 4 bytes     │
  │ (hash of │            │          │          │ (of the  │ CRC32C of   │
  │  URL)    │            │          │          │  chunk)  │ this record │
  └──────────┴────────────┴──────────┴──────────┴──────────┴─────────────┘
  record_hash = CRC32C(file_id + chunk_idx + offset + size + crc32c)
  This protects the WAL entry itself from corruption.
  If record_hash is bad → this WAL entry is untrustworthy → file must be re-downloaded.

File Entry (36 bytes):
  ┌──────────┬──────────┬──────────┬──────────┬──────────┬─────────────┐
  │ file_id  │ status   │ total    │ file_crc │ reserved │ record_hash │
  │ 8 bytes  │ 4 bytes  │ 4 bytes  │ 4 bytes  │ 8 bytes  │ 4 bytes     │
  │          │ COMPLETE │ chunks   │ (whole   │          │ CRC32C of   │
  │          │ VERIFIED │ count    │  file)   │          │ this record │
  └──────────┴──────────┴──────────┴──────────┴──────────┴─────────────┘

WAL integrity guarantee:
  Every record is self-verifiable via record_hash.
  If any record_hash mismatches → that file is untrusted → re-download.
  The WAL is the source of truth, so it MUST be correct.

Size math:
  500 MB file ÷ 1 MB chunks = 500 chunk entries × 36 bytes = 18 KB
  + 1 file entry = 36 bytes
  Total WAL for one file: ~18 KB
  fsync'ing 18 KB on eMMC: <5ms (vs 50-200ms for the 500 MB data)
```

---

## 5. WAL Lifecycle

```
  NOTE: WAL NEVER stores actual file data. Only checksums + metadata.
  This is what makes WAL fsync cheap on eMMC.

  ┌──────────────────────────────────────────────────────────────┐
  │                                                              │
  │  WAL BUFFER (in memory)                                     │
  │  ┌────────────────────────────────────────────────────────┐ │
  │  │  chunk entries accumulate during download              │ │
  │  │  [chunk0][chunk1][chunk2]...[chunk499][FILE_COMPLETE]  │ │
  │  └────────────────────────────────────────────────────────┘ │
  │       │                                                      │
  │       │ Periodic flush + fsync (configurable):               │
  │       │   Option A: on download complete (default)           │
  │       │   Option B: every N seconds (e.g., 5s) for long     │
  │       │             downloads, to limit data loss window     │
  │       │                                                      │
  │       │ Even Option B is cheap: WAL is ~36 bytes/chunk.     │
  │       │ fsync'ing a few KB every 5 seconds = negligible.     │
  │       ▼                                                      │
  │  WAL FILE (on disk, durable after fsync)                    │
  │  ┌────────────────────────────────────────────────────────┐ │
  │  │  durable checksum records for all downloads            │ │
  │  │  keeps growing as more files are downloaded            │ │
  │  │  every record has its own record_hash for integrity    │ │
  │  └────────────────────────────────────────────────────────┘ │
  │       │                                                      │
  │       │ after file reaches VERIFIED status                   │
  │       ▼                                                      │
  │  GC (Garbage Collection)                                    │
  │  ┌────────────────────────────────────────────────────────┐ │
  │  │  Periodic GC cleans up both data files and WAL:        │ │
  │  │                                                        │ │
  │  │  1. Scan data directory                                │ │
  │  │     For each file on disk:                             │ │
  │  │       if no WAL entry exists → orphan → delete file    │ │
  │  │     (catches: WAL never fsync'd, incomplete downloads) │ │
  │  │                                                        │ │
  │  │  2. Scan WAL entries                                   │ │
  │  │     For each file_id in WAL:                           │ │
  │  │       if data file no longer exists → stale → remove   │ │
  │  │         WAL entries for that file_id                   │ │
  │  │     (catches: user deleted file, corruption cleanup)   │ │
  │  │                                                        │ │
  │  │  3. Compact WAL                                        │ │
  │  │     - Write new WAL with only live entries             │ │
  │  │     - fsync new WAL file                               │ │
  │  │     - Rename new → old (atomic on most filesystems)    │ │
  │  │                                                        │ │
  │  │  Rule: WAL entry lives as long as the data file.       │ │
  │  │        File deleted → WAL entries cleaned by GC.       │ │
  │  │        File exists → WAL entries MUST exist (for       │ │
  │  │        verify cycle to work).                          │ │
  │  └────────────────────────────────────────────────────────┘ │
  │                                                              │
  └──────────────────────────────────────────────────────────────┘
```

---

## 6. fsync Budget

```
The whole point: minimize fsyncs on slow eMMC.

┌────────────────────────┬───────────┬──────────────────────────────┐
│ Event                  │ fsyncs    │ What's being fsync'd         │
├────────────────────────┼───────────┼──────────────────────────────┤
│ Each chunk written     │ 0         │ Nothing (data file, no sync) │
│ Download complete      │ 1         │ WAL file (~18 KB)            │
│ Verification complete  │ 1         │ WAL file (status update)     │
│ WAL compaction         │ 1         │ New compacted WAL file       │
├────────────────────────┼───────────┼──────────────────────────────┤
│ TOTAL per file         │ 2-3       │ All on tiny WAL, never on    │
│                        │           │ the large data file          │
└────────────────────────┴───────────┴──────────────────────────────┘

Compare with naive approach:
  fsync per chunk: 500 fsyncs × 100ms = 50 SECONDS of just fsync
  WAL approach:    2 fsyncs × 5ms = 10ms of fsync

  That's 5000× fewer fsyncs.

Data files eventually reach disk through normal OS writeback
(dirty page expiry, typically 30 seconds on Linux: vm.dirty_expire_centisecs).
We just don't FORCE it — we let the OS decide when.
```

---

## 7. State Machine

```
                    ┌──────────────┐
                    │ DOWNLOADING  │
                    │              │
                    │ chunks       │
                    │ streaming,   │
                    │ checksums in │
                    │ WAL buffer   │
                    └──────┬───────┘
                           │ all chunks received
                           │ WAL buffer flushed + fsync'd
                           ▼
                    ┌──────────────┐
                    │  COMPLETE    │
                    │              │
                    │ checksums    │
                    │ durable in   │
                    │ WAL on disk  │
                    └──────┬───────┘
                           │ verify cycle (every verify_interval)
                           │ reads all chunks, compares CRC vs WAL
                           │
                     ┌─────┴─────┐
                     │           │
              all match      any mismatch
                     │           │
                     ▼           ▼
              ┌────────────┐  ┌───────────────┐
              │  VERIFIED  │  │ CORRUPT       │
              │            │  │               │
              │  trusted   │  │ delete file   │
              │  until     │  │ re-download   │
              │  next      │  │ from scratch  │──→ DOWNLOADING
              │  verify    │  │               │
              │  cycle     │  └───────────────┘
              └──────┬─────┘
                     │ next verify cycle
                     │ (re-checks periodically)
                     ▼
              (back to COMPLETE for re-verification)
```

---

## 8. Edge Cases

```
┌──────────────────────────────────┬────────────────────────────────┐
│ Scenario                         │ What happens                   │
├──────────────────────────────────┼────────────────────────────────┤
│ Power loss during download       │ WAL buffer not yet fsync'd.    │
│ (before WAL fsync)               │ On restart: no WAL record →    │
│                                  │ delete partial data file,      │
│                                  │ restart download from scratch. │
├──────────────────────────────────┼────────────────────────────────┤
│ Power loss after WAL fsync       │ WAL has all checksums.         │
│ but before verification          │ Data file may be partially     │
│                                  │ flushed by OS. On restart:     │
│                                  │ next verify cycle detects      │
│                                  │ corruption → re-download.      │
├──────────────────────────────────┼────────────────────────────────┤
│ Bit rot months later             │ Periodic verify cycle catches  │
│                                  │ it within verify_interval.     │
│                                  │ File deleted, re-downloaded.   │
├──────────────────────────────────┼────────────────────────────────┤
│ WAL record_hash mismatch         │ WAL entry is corrupt.          │
│                                  │ File treated as untrusted →    │
│                                  │ delete data, re-download,      │
│                                  │ rebuild WAL entries.           │
├──────────────────────────────────┼────────────────────────────────┤
│ WAL header corrupt               │ Entire WAL untrustworthy.      │
│                                  │ Delete all data files,         │
│                                  │ re-download everything,        │
│                                  │ rebuild WAL from scratch.      │
├──────────────────────────────────┼────────────────────────────────┤
│ eMMC wear (too many writes)      │ WAL is tiny (~KB per file).    │
│                                  │ Compaction keeps it small.     │
│                                  │ eMMC wear-leveling handles it. │
├──────────────────────────────────┼────────────────────────────────┤
│ Same file re-downloaded          │ Old WAL entries replaced.      │
│ (URL unchanged, content changed) │ New checksums from fresh       │
│                                  │ download. Next verify confirms.│
└──────────────────────────────────┴────────────────────────────────┘
```

---

## 9. Chunk Size Tradeoff

```
Chunk size affects WAL size and detection granularity.
On corruption, we re-download the ENTIRE file (no partial repair).
So chunk size is about WAL overhead, not repair cost.

┌────────────┬──────────┬──────────────────────────────────────────┐
│ Chunk size │ WAL size │ Notes                                    │
│            │ per 500MB│                                          │
├────────────┼──────────┼──────────────────────────────────────────┤
│ 64 KB      │ 282 KB   │ Very precise detection, but large WAL.   │
│            │          │ More entries = more record_hash checks.   │
│ 256 KB     │ 70 KB    │ Decent balance.                          │
│ 1 MB       │ 18 KB    │ Small WAL, fast verify cycle.            │
│ 4 MB       │ 4.5 KB   │ Minimal WAL, but coarse detection.       │
└────────────┴──────────┴──────────────────────────────────────────┘

Recommendation for eMMC: 1 MB chunks.
  - WAL stays tiny (18 KB per 500 MB file)
  - Verify cycle reads file sequentially — 1 MB reads are efficient on eMMC
  - Aligns well with eMMC erase block sizes (typically 512 KB - 4 MB)
  - Fewer pwrite() calls during download = less eMMC wear
  - Corruption is pinpointed to 1 MB granularity in logs
    (even though we re-download the whole file)
```

---

## 10. Why Not Just Use the HTTP ETag/Content-MD5?

```
You might think: "just check the server's ETag after download."

  Problem 1: ETag only covers the WHOLE file.
    You can only verify by re-downloading the entire file from the server.
    With local per-chunk checksums, you verify locally in seconds — no network.

  Problem 2: ETag doesn't detect post-download corruption.
    File was correct at download time, but bit rot happened later.
    You'd need to re-download the entire file just to check.
    With local WAL checksums, you verify locally in seconds.

  Problem 3: ETags aren't always content hashes.
    S3 multipart upload ETags are NOT MD5 of the content.
    Many CDNs return opaque ETags. Can't rely on them.

  The WAL checksum is a LOCAL source of truth:
    "These are the exact bytes I received from the network.
     I can verify the disk still has them, anytime, without network."
```
