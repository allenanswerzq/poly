//! # File Storage (Dropbox) - Mini Implementation
//!
//! Demonstrates:
//! - Content-addressable storage (chunking + hashing)
//! - Deduplication
//! - Sync protocol (delta sync)
//! - File versioning
//!
//! Run: cargo run -p file-storage

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

// =============================================================================
// Core Types
// =============================================================================

const CHUNK_SIZE: usize = 4096; // 4KB chunks

#[derive(Debug, Clone)]
struct FileMetadata {
    path: String,
    size: u64,
    chunks: Vec<String>, // Chunk hashes
    version: u64,
    modified_at: u64,
    created_at: u64,
}

#[derive(Debug, Clone)]
struct Chunk {
    hash: String,
    data: Vec<u8>,
    references: u64, // Reference count for dedup
}

#[derive(Debug, Clone)]
struct SyncState {
    cursor: u64,
    local_files: HashMap<String, u64>, // path -> version
}

#[derive(Debug, Clone)]
enum SyncChange {
    FileAdded(FileMetadata),
    FileModified(FileMetadata),
    FileDeleted(String),
}

// =============================================================================
// Content-Addressable Storage
// =============================================================================

struct ChunkStore {
    chunks: DashMap<String, Chunk>,
    total_size: AtomicU64,
    dedup_savings: AtomicU64,
}

impl ChunkStore {
    fn new() -> Self {
        Self {
            chunks: DashMap::new(),
            total_size: AtomicU64::new(0),
            dedup_savings: AtomicU64::new(0),
        }
    }

    fn hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn store(&self, data: Vec<u8>) -> String {
        let hash = Self::hash(&data);
        let size = data.len() as u64;

        if let Some(mut chunk) = self.chunks.get_mut(&hash) {
            // Already exists - increment reference
            chunk.references += 1;
            self.dedup_savings.fetch_add(size, Ordering::SeqCst);
            return hash;
        }

        // New chunk
        self.chunks.insert(
            hash.clone(),
            Chunk {
                hash: hash.clone(),
                data,
                references: 1,
            },
        );
        self.total_size.fetch_add(size, Ordering::SeqCst);

        hash
    }

    fn get(&self, hash: &str) -> Option<Vec<u8>> {
        self.chunks.get(hash).map(|c| c.data.clone())
    }

    fn release(&self, hash: &str) {
        if let Some(mut chunk) = self.chunks.get_mut(hash) {
            chunk.references -= 1;
            if chunk.references == 0 {
                let size = chunk.data.len() as u64;
                drop(chunk);
                self.chunks.remove(hash);
                self.total_size.fetch_sub(size, Ordering::SeqCst);
            }
        }
    }

    fn stats(&self) -> (u64, u64, usize) {
        (
            self.total_size.load(Ordering::SeqCst),
            self.dedup_savings.load(Ordering::SeqCst),
            self.chunks.len(),
        )
    }
}

// =============================================================================
// File System
// =============================================================================

struct FileSystem {
    files: DashMap<String, FileMetadata>,
    versions: DashMap<String, VecDeque<FileMetadata>>, // path -> version history
    chunk_store: ChunkStore,
    version_counter: AtomicU64,
    max_versions: usize,
}

impl FileSystem {
    fn new() -> Self {
        Self {
            files: DashMap::new(),
            versions: DashMap::new(),
            chunk_store: ChunkStore::new(),
            version_counter: AtomicU64::new(0),
            max_versions: 10,
        }
    }

    fn upload(&self, path: &str, data: &[u8]) -> FileMetadata {
        // Chunk the file
        let chunks: Vec<String> = data
            .chunks(CHUNK_SIZE)
            .map(|chunk| self.chunk_store.store(chunk.to_vec()))
            .collect();

        let version = self.version_counter.fetch_add(1, Ordering::SeqCst) + 1;

        let metadata = FileMetadata {
            path: path.to_string(),
            size: data.len() as u64,
            chunks,
            version,
            modified_at: version, // Using version as timestamp for simplicity
            created_at: version,
        };

        // Store version history
        if let Some(old) = self.files.insert(path.to_string(), metadata.clone()) {
            // Release old chunks
            for chunk_hash in &old.chunks {
                self.chunk_store.release(chunk_hash);
            }

            // Add to version history
            let mut history = self.versions.entry(path.to_string()).or_default();
            history.push_back(old);
            if history.len() > self.max_versions {
                if let Some(oldest) = history.pop_front() {
                    for chunk_hash in &oldest.chunks {
                        self.chunk_store.release(chunk_hash);
                    }
                }
            }
        }

        metadata
    }

    fn download(&self, path: &str) -> Option<Vec<u8>> {
        let metadata = self.files.get(path)?;

        let mut data = Vec::with_capacity(metadata.size as usize);
        for chunk_hash in &metadata.chunks {
            data.extend(self.chunk_store.get(chunk_hash)?);
        }

        Some(data)
    }

    fn delete(&self, path: &str) -> Option<FileMetadata> {
        if let Some((_, metadata)) = self.files.remove(path) {
            for chunk_hash in &metadata.chunks {
                self.chunk_store.release(chunk_hash);
            }
            return Some(metadata);
        }
        None
    }

    fn get_metadata(&self, path: &str) -> Option<FileMetadata> {
        self.files.get(path).map(|f| f.clone())
    }

    fn list(&self, prefix: &str) -> Vec<FileMetadata> {
        self.files
            .iter()
            .filter(|e| e.path.starts_with(prefix))
            .map(|e| e.value().clone())
            .collect()
    }

    fn get_version(&self, path: &str, version: u64) -> Option<FileMetadata> {
        // Check current version
        if let Some(current) = self.files.get(path) {
            if current.version == version {
                return Some(current.clone());
            }
        }

        // Check history
        self.versions.get(path).and_then(|history| {
            history.iter().find(|m| m.version == version).cloned()
        })
    }

    fn restore_version(&self, path: &str, version: u64) -> Option<FileMetadata> {
        let old_version = self.get_version(path, version)?;

        // Re-upload with same data
        let data = self.reconstruct_version(&old_version)?;
        Some(self.upload(path, &data))
    }

    fn reconstruct_version(&self, metadata: &FileMetadata) -> Option<Vec<u8>> {
        let mut data = Vec::new();
        for chunk_hash in &metadata.chunks {
            data.extend(self.chunk_store.get(chunk_hash)?);
        }
        Some(data)
    }
}

// =============================================================================
// Sync Service
// =============================================================================

struct SyncService {
    fs: FileSystem,
    change_log: Mutex<Vec<(u64, SyncChange)>>, // (version, change)
    cursor: AtomicU64,
}

impl SyncService {
    fn new() -> Self {
        Self {
            fs: FileSystem::new(),
            change_log: Mutex::new(Vec::new()),
            cursor: AtomicU64::new(0),
        }
    }

    fn upload(&self, path: &str, data: &[u8]) -> FileMetadata {
        let metadata = self.fs.upload(path, data);
        let cursor = self.cursor.fetch_add(1, Ordering::SeqCst) + 1;

        let change = if self.fs.get_metadata(path).map(|m| m.version).unwrap_or(0) == metadata.version {
            SyncChange::FileAdded(metadata.clone())
        } else {
            SyncChange::FileModified(metadata.clone())
        };

        self.change_log.lock().push((cursor, change));
        metadata
    }

    fn delete(&self, path: &str) -> Option<FileMetadata> {
        if let Some(metadata) = self.fs.delete(path) {
            let cursor = self.cursor.fetch_add(1, Ordering::SeqCst) + 1;
            self.change_log
                .lock()
                .push((cursor, SyncChange::FileDeleted(path.to_string())));
            return Some(metadata);
        }
        None
    }

    fn get_changes(&self, since_cursor: u64) -> (u64, Vec<SyncChange>) {
        let log = self.change_log.lock();
        let changes: Vec<SyncChange> = log
            .iter()
            .filter(|(cursor, _)| *cursor > since_cursor)
            .map(|(_, change)| change.clone())
            .collect();

        let new_cursor = self.cursor.load(Ordering::SeqCst);
        (new_cursor, changes)
    }

    fn delta_chunks(&self, path: &str, local_chunks: &[String]) -> Vec<String> {
        // Return chunks that remote has but local doesn't
        if let Some(metadata) = self.fs.get_metadata(path) {
            return metadata
                .chunks
                .iter()
                .filter(|hash| !local_chunks.contains(hash))
                .cloned()
                .collect();
        }
        Vec::new()
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== File Storage (Dropbox) Demo ===\n");

    let service = SyncService::new();

    // Upload files
    println!("\n  ═══ Uploading Files ═══");

    let file1_data = b"Hello, World! This is my first file.".to_vec();
    let meta1 = service.upload("/documents/hello.txt", &file1_data);
    println!(
        "Uploaded: {} ({} bytes, {} chunks, v{})",
        meta1.path,
        meta1.size,
        meta1.chunks.len(),
        meta1.version
    );

    // Upload duplicate content (deduplication test)
    let file2_data = b"Hello, World! This is my first file.".to_vec(); // Same content
    let meta2 = service.upload("/backup/hello.txt", &file2_data);
    println!(
        "Uploaded: {} ({} bytes, {} chunks, v{})",
        meta2.path,
        meta2.size,
        meta2.chunks.len(),
        meta2.version
    );

    // Upload larger file with chunking
    let large_data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let meta3 = service.upload("/documents/large.bin", &large_data);
    println!(
        "Uploaded: {} ({} bytes, {} chunks, v{})",
        meta3.path,
        meta3.size,
        meta3.chunks.len(),
        meta3.version
    );

    // Storage stats
    let (total, dedup_savings, chunk_count) = service.fs.chunk_store.stats();
    println!("\nStorage stats:");
    println!("  Total storage: {} bytes", total);
    println!("  Dedup savings: {} bytes", dedup_savings);
    println!("  Unique chunks: {}", chunk_count);
    println!();

    // Download file
    println!("\n  ═══ Downloading ═══");
    let downloaded = service.fs.download("/documents/hello.txt").unwrap();
    println!(
        "Downloaded {} bytes: '{}'",
        downloaded.len(),
        String::from_utf8_lossy(&downloaded[..50.min(downloaded.len())])
    );
    println!();

    // File modification (versioning)
    println!("\n  ═══ File Versioning ═══");
    let v1 = service.fs.get_metadata("/documents/hello.txt").unwrap().version;
    println!("Current version: {}", v1);

    service.upload("/documents/hello.txt", b"Updated content!");
    let v2 = service.fs.get_metadata("/documents/hello.txt").unwrap().version;
    println!("After update: v{}", v2);

    // List versions
    if let Some(history) = service.fs.versions.get("/documents/hello.txt") {
        println!("Version history: {} versions", history.len());
    }

    // Restore old version
    service.fs.restore_version("/documents/hello.txt", v1);
    let restored = service.fs.download("/documents/hello.txt").unwrap();
    println!(
        "Restored v{}: '{}'",
        v1,
        String::from_utf8_lossy(&restored)
    );
    println!();

    // Sync protocol
    println!("\n  ═══ Sync Protocol ═══");

    // Client has cursor 0 (never synced)
    let (new_cursor, changes) = service.get_changes(0);
    println!("Changes since cursor 0: {} changes", changes.len());
    for change in &changes {
        match change {
            SyncChange::FileAdded(m) => println!("  + {}", m.path),
            SyncChange::FileModified(m) => println!("  ~ {}", m.path),
            SyncChange::FileDeleted(p) => println!("  - {}", p),
        }
    }
    println!("New cursor: {}", new_cursor);

    // Delta sync
    println!("\n--- Delta Sync ---");
    let local_chunks: Vec<String> = meta3.chunks[0..1].to_vec(); // Pretend client has first chunk
    let needed = service.delta_chunks("/documents/large.bin", &local_chunks);
    println!(
        "Client has {} chunks, needs {} more",
        local_chunks.len(),
        needed.len()
    );

    // Delete file
    println!("\n--- Delete ---");
    service.delete("/backup/hello.txt");

    let (_, changes) = service.get_changes(new_cursor);
    println!("New changes: {:?}", changes.len());

    // List files
    println!("\n--- List Files ---");
    for file in service.fs.list("/") {
        println!("  {} ({} bytes)", file.path, file.size);
    }

    println!("\n=== Key Concepts ===");
    println!("1. Chunking: Split files into fixed-size chunks");
    println!("2. Content-Addressable: Store by hash, deduplicate");
    println!("3. Versioning: Keep history of file versions");
    println!("4. Delta Sync: Only transfer changed chunks");
    println!("5. Change Log: Track changes with cursor for sync");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplication() {
        let store = ChunkStore::new();

        let data = vec![1, 2, 3, 4, 5];
        let hash1 = store.store(data.clone());
        let hash2 = store.store(data.clone());

        assert_eq!(hash1, hash2);
        assert_eq!(store.chunks.len(), 1);

        let (_, savings, _) = store.stats();
        assert_eq!(savings, 5);
    }

    #[test]
    fn test_chunking() {
        let fs = FileSystem::new();

        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let meta = fs.upload("/test", &data);

        assert!(meta.chunks.len() > 1);

        let downloaded = fs.download("/test").unwrap();
        assert_eq!(downloaded, data);
    }

    #[test]
    fn test_versioning() {
        let fs = FileSystem::new();

        fs.upload("/test", b"version 1");
        let v1 = fs.get_metadata("/test").unwrap().version;

        fs.upload("/test", b"version 2");
        let v2 = fs.get_metadata("/test").unwrap().version;

        assert!(v2 > v1);

        // Can get old version
        let old = fs.get_version("/test", v1);
        assert!(old.is_some());
    }
}
