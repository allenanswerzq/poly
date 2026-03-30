#![allow(dead_code, unused_variables, unused_imports)]
//! # Merkle Tree & Content-Addressable Storage
//!
//! Hash trees where each node is the hash of its children.
//! Any change in data propagates upward → tamper detection + efficient sync.
//!
//! Used in: Git, IPFS, Bitcoin/Ethereum, Amazon Dynamo (anti-entropy),
//! Cassandra (repair), certificate transparency, file deduplication.

use sha2::{Digest, Sha256};
use std::fmt;

// =============================================================================
// Merkle Tree
// =============================================================================

type Hash = [u8; 32];

fn hash_leaf(data: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([0x00]); // domain separation: leaf prefix
    hasher.update(data);
    hasher.finalize().into()
}

fn hash_internal(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update([0x01]); // domain separation: internal node prefix
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn hash_to_hex(h: &Hash) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect::<String>()
}

fn short_hash(h: &Hash) -> String {
    hash_to_hex(h)[..12].to_string()
}

pub struct MerkleTree {
    /// All nodes stored level by level. Root is at index 0.
    /// For n leaves, tree has ~2n nodes.
    nodes: Vec<Hash>,
    leaf_count: usize,
}

impl MerkleTree {
    /// Build a Merkle tree from data blocks.
    pub fn build(data: &[&[u8]]) -> Self {
        if data.is_empty() {
            return Self {
                nodes: vec![[0u8; 32]],
                leaf_count: 0,
            };
        }

        // Hash all leaves
        let mut leaves: Vec<Hash> = data.iter().map(|d| hash_leaf(d)).collect();

        // Pad to even number by duplicating last leaf
        if leaves.len() % 2 == 1 {
            leaves.push(*leaves.last().unwrap());
        }

        let leaf_count = data.len();

        // Build tree bottom-up
        let mut level = leaves.clone();
        let mut all_nodes = Vec::new();

        while level.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in level.chunks(2) {
                let left = &chunk[0];
                let right = if chunk.len() > 1 { &chunk[1] } else { left };
                next_level.push(hash_internal(left, right));
            }
            all_nodes.push(level);
            level = next_level;
            if level.len() % 2 == 1 && level.len() > 1 {
                level.push(*level.last().unwrap());
            }
        }
        all_nodes.push(level); // root level

        // Flatten: root first, then levels
        let mut nodes = Vec::new();
        for level in all_nodes.iter().rev() {
            nodes.extend_from_slice(level);
        }

        Self { nodes, leaf_count }
    }

    /// Root hash — the fingerprint of all data.
    pub fn root_hash(&self) -> &Hash {
        &self.nodes[0]
    }

    /// Generate a proof that a leaf at `index` is part of the tree.
    /// Returns sibling hashes from leaf to root.
    pub fn proof(&self, index: usize) -> Vec<(Hash, bool)> {
        // Rebuild leaf hashes and compute proof path
        // For simplicity, we'll recompute the proof from scratch
        // In production you'd store the tree structure properly
        Vec::new() // simplified — see verify for the concept
    }

    /// Verify data integrity: check if root matches expected.
    pub fn verify(data: &[&[u8]], expected_root: &Hash) -> bool {
        let tree = Self::build(data);
        tree.root_hash() == expected_root
    }
}

// =============================================================================
// Content-Addressable Storage
// =============================================================================
// Store data by its hash. Deduplication is free — same content → same key.
// Used in: Git objects, IPFS blocks, Docker layers, backup systems.

pub struct ContentStore {
    store: std::collections::HashMap<String, Vec<u8>>, // hash_hex → data
}

impl ContentStore {
    pub fn new() -> Self {
        Self {
            store: std::collections::HashMap::new(),
        }
    }

    /// Store data and return its content hash.
    pub fn put(&mut self, data: &[u8]) -> String {
        let hash = hash_leaf(data);
        let hex = hash_to_hex(&hash);
        self.store.entry(hex.clone()).or_insert_with(|| data.to_vec());
        hex
    }

    /// Retrieve data by its hash.
    pub fn get(&self, hash: &str) -> Option<&[u8]> {
        self.store.get(hash).map(|v| v.as_slice())
    }

    /// Check if content exists (without fetching).
    pub fn contains(&self, hash: &str) -> bool {
        self.store.contains_key(hash)
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Total bytes stored.
    pub fn total_bytes(&self) -> usize {
        self.store.values().map(|v| v.len()).sum()
    }
}

// =============================================================================
// Merkle-based Anti-Entropy (Dynamo/Cassandra style)
// =============================================================================
// Two nodes compare Merkle roots. If they differ, recursively compare
// subtrees to find the minimal set of differing data blocks.

pub fn find_differences(data_a: &[&[u8]], data_b: &[&[u8]]) -> Vec<usize> {
    let mut diffs = Vec::new();

    if data_a.len() != data_b.len() {
        // Different sizes — compare what we can, mark rest as different
        let min_len = data_a.len().min(data_b.len());
        for i in 0..min_len {
            if hash_leaf(data_a[i]) != hash_leaf(data_b[i]) {
                diffs.push(i);
            }
        }
        for i in min_len..data_a.len().max(data_b.len()) {
            diffs.push(i);
        }
        return diffs;
    }

    // Same size: build trees and compare roots
    let tree_a = MerkleTree::build(data_a);
    let tree_b = MerkleTree::build(data_b);

    if tree_a.root_hash() == tree_b.root_hash() {
        return diffs; // identical
    }

    // Roots differ → compare leaf by leaf (simplified; real impl compares subtrees)
    for i in 0..data_a.len() {
        if hash_leaf(data_a[i]) != hash_leaf(data_b[i]) {
            diffs.push(i);
        }
    }

    diffs
}

// =============================================================================
// Hash Chain (simple blockchain concept)
// =============================================================================

pub struct HashChain {
    blocks: Vec<(Hash, Vec<u8>)>, // (prev_hash, data)
}

impl HashChain {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn append(&mut self, data: &[u8]) -> Hash {
        let prev = if self.blocks.is_empty() {
            [0u8; 32]
        } else {
            self.block_hash(self.blocks.len() - 1)
        };

        self.blocks.push((prev, data.to_vec()));
        self.block_hash(self.blocks.len() - 1)
    }

    fn block_hash(&self, index: usize) -> Hash {
        let (prev, data) = &self.blocks[index];
        let mut hasher = Sha256::new();
        hasher.update(prev);
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Verify chain integrity: each block's prev_hash matches prior block's hash.
    pub fn verify(&self) -> bool {
        for i in 1..self.blocks.len() {
            let expected_prev = self.block_hash(i - 1);
            if self.blocks[i].0 != expected_prev {
                return false;
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn latest_hash(&self) -> Option<Hash> {
        if self.blocks.is_empty() {
            return None;
        }
        Some(self.block_hash(self.blocks.len() - 1))
    }
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Merkle Tree ===\n");

    let data: Vec<&[u8]> = vec![b"block-0", b"block-1", b"block-2", b"block-3"];
    let tree = MerkleTree::build(&data);
    println!("Data blocks: {:?}", data.iter().map(|d| std::str::from_utf8(d).unwrap()).collect::<Vec<_>>());
    println!("Root hash: {}", short_hash(tree.root_hash()));

    // Tamper detection
    let tampered: Vec<&[u8]> = vec![b"block-0", b"TAMPERED", b"block-2", b"block-3"];
    let tampered_tree = MerkleTree::build(&tampered);
    println!("Tampered root: {}", short_hash(tampered_tree.root_hash()));
    println!("Roots match: {}", tree.root_hash() == tampered_tree.root_hash());

    // Verify
    println!("Verify original: {}", MerkleTree::verify(&data, tree.root_hash()));
    println!("Verify tampered: {}", MerkleTree::verify(&tampered, tree.root_hash()));

    println!("\n=== Content-Addressable Storage ===\n");
    let mut cas = ContentStore::new();
    let h1 = cas.put(b"hello world");
    let h2 = cas.put(b"hello world"); // duplicate!
    let h3 = cas.put(b"different data");

    println!("h1: {}...", &h1[..24]);
    println!("h2: {}...", &h2[..24]);
    println!("h3: {}...", &h3[..24]);
    println!("h1 == h2 (dedup): {}", h1 == h2);
    println!("Unique items stored: {} (inserted 3, 1 duplicate)", cas.len());
    println!("Retrieved: {:?}", cas.get(&h1).map(|d| std::str::from_utf8(d).unwrap()));

    println!("\n=== Anti-Entropy (find differences) ===\n");
    let node_a: Vec<&[u8]> = vec![b"data-0", b"data-1", b"data-2", b"data-3"];
    let node_b: Vec<&[u8]> = vec![b"data-0", b"STALE-1", b"data-2", b"STALE-3"];
    let diffs = find_differences(&node_a, &node_b);
    println!("Node A: {:?}", node_a.iter().map(|d| std::str::from_utf8(d).unwrap()).collect::<Vec<_>>());
    println!("Node B: {:?}", node_b.iter().map(|d| std::str::from_utf8(d).unwrap()).collect::<Vec<_>>());
    println!("Differing blocks: {:?}", diffs);
    println!("→ Only need to sync {} out of {} blocks", diffs.len(), node_a.len());

    println!("\n=== Hash Chain ===\n");
    let mut chain = HashChain::new();
    chain.append(b"genesis block");
    chain.append(b"transaction: Alice -> Bob 10");
    chain.append(b"transaction: Bob -> Charlie 5");

    println!("Chain length: {}", chain.len());
    println!("Latest hash: {}", short_hash(&chain.latest_hash().unwrap()));
    println!("Chain valid: {}", chain.verify());
}
