//! Chain State Module
//!
//! This module tracks the canonical blockchain state, managing:
//! - Block storage and retrieval
//! - Chain reorganizations (reorgs)
//! - Fork choice rule implementation
//! - State transitions
//!
//! In Ethereum, the chain state is the source of truth that all other
//! components reference. It determines which blocks are valid and which
//! branch of the chain is considered canonical.

use std::collections::{HashMap, HashSet, VecDeque};
use crate::error::BuilderError;

/// A simple block hash type (32 bytes)
pub type BlockHash = [u8; 32];

/// A block number (height in the chain)
pub type BlockNumber = u64;

/// Represents a complete block in the chain
#[derive(Debug, Clone)]
pub struct Block {
    /// Block header hash (unique identifier)
    pub hash: BlockHash,
    
    /// Parent block's hash
    pub parent_hash: BlockHash,
    
    /// Block number (height from genesis)
    pub number: BlockNumber,
    
    /// State root after executing this block
    pub state_root: BlockHash,
    
    /// Unix timestamp of when block was created
    pub timestamp: u64,
    
    /// Address of the block proposer/builder
    pub coinbase: [u8; 20],
    
    /// Total difficulty up to and including this block
    /// (legacy, but useful for fork choice)
    pub total_difficulty: u128,
    
    /// Gas used by all transactions in block
    pub gas_used: u64,
    
    /// Maximum gas allowed in this block
    pub gas_limit: u64,
    
    /// Base fee per gas (EIP-1559)
    pub base_fee: u128,
    
    /// Transaction hashes included in this block
    pub transaction_hashes: Vec<BlockHash>,
    
    /// Extra data (builder signature, graffiti, etc.)
    pub extra_data: Vec<u8>,
}

impl Block {
    /// Create a genesis block (block 0)
    pub fn genesis() -> Self {
        Block {
            hash: [0u8; 32], // Will be computed
            parent_hash: [0u8; 32], // Genesis has no parent
            number: 0,
            state_root: [0u8; 32],
            timestamp: 1606824023, // Ethereum mainnet genesis timestamp
            coinbase: [0u8; 20],
            total_difficulty: 0,
            gas_used: 0,
            gas_limit: 30_000_000,
            base_fee: 1_000_000_000, // 1 Gwei
            transaction_hashes: vec![],
            extra_data: b"Genesis Block".to_vec(),
        }
    }
    
    /// Check if this is the genesis block
    pub fn is_genesis(&self) -> bool {
        self.number == 0 && self.parent_hash == [0u8; 32]
    }
    
    /// Compute a simple hash for the block
    /// In real Ethereum, this would be keccak256(RLP(header))
    pub fn compute_hash(&self) -> BlockHash {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        self.parent_hash.hash(&mut hasher);
        self.number.hash(&mut hasher);
        self.timestamp.hash(&mut hasher);
        self.state_root.hash(&mut hasher);
        
        let hash_value = hasher.finish();
        let mut result = [0u8; 32];
        result[..8].copy_from_slice(&hash_value.to_be_bytes());
        result[8..16].copy_from_slice(&self.number.to_be_bytes());
        result
    }
}

/// Fork choice rules determine which chain is canonical
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkChoiceRule {
    /// Longest chain wins (original PoW rule)
    LongestChain,
    
    /// Heaviest chain wins (total difficulty)
    HeaviestChain,
    
    /// Latest justified checkpoint wins (PoS/Casper)
    LatestJustified,
}

/// Represents a potential fork in the chain
#[derive(Debug, Clone)]
pub struct Fork {
    /// Where the fork diverges from canonical chain
    pub fork_point: BlockHash,
    
    /// The tip of the fork
    pub tip: BlockHash,
    
    /// Number of blocks in this fork
    pub length: u64,
    
    /// Total difficulty of this fork
    pub total_difficulty: u128,
}

/// Chain statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct ChainStats {
    /// Total number of blocks stored
    pub total_blocks: u64,
    
    /// Current canonical chain length
    pub chain_height: BlockNumber,
    
    /// Number of reorgs that have occurred
    pub reorg_count: u64,
    
    /// Deepest reorg depth seen
    pub max_reorg_depth: u64,
    
    /// Number of orphaned blocks
    pub orphan_count: u64,
}

/// The main chain state manager
///
/// This manages the blockchain's state, including:
/// - All blocks (canonical and non-canonical)
/// - The canonical chain
/// - Pending blocks waiting for parents
/// - Fork detection and handling
pub struct ChainState {
    /// All blocks indexed by hash
    blocks: HashMap<BlockHash, Block>,
    
    /// Block hash at each height on canonical chain
    canonical_chain: HashMap<BlockNumber, BlockHash>,
    
    /// Head of the canonical chain
    head: Option<BlockHash>,
    
    /// Blocks waiting for their parent (orphans)
    orphan_blocks: HashMap<BlockHash, Block>,
    
    /// Child blocks for each parent
    children: HashMap<BlockHash, HashSet<BlockHash>>,
    
    /// Fork choice rule in use
    fork_choice_rule: ForkChoiceRule,
    
    /// Chain statistics
    stats: ChainStats,
    
    /// Maximum reorg depth we allow
    max_reorg_depth: u64,
}

impl ChainState {
    /// Create a new chain state with genesis block
    pub fn new(fork_choice_rule: ForkChoiceRule) -> Self {
        let mut chain = ChainState {
            blocks: HashMap::new(),
            canonical_chain: HashMap::new(),
            head: None,
            orphan_blocks: HashMap::new(),
            children: HashMap::new(),
            fork_choice_rule,
            stats: ChainStats::default(),
            max_reorg_depth: 64, // Similar to Ethereum's finality depth
        };
        
        // Add genesis block
        let mut genesis = Block::genesis();
        genesis.hash = genesis.compute_hash();
        chain.insert_genesis(genesis);
        
        chain
    }
    
    /// Insert the genesis block
    fn insert_genesis(&mut self, genesis: Block) {
        let hash = genesis.hash;
        self.blocks.insert(hash, genesis);
        self.canonical_chain.insert(0, hash);
        self.head = Some(hash);
        self.stats.total_blocks = 1;
        self.stats.chain_height = 0;
    }
    
    /// Get a block by hash
    pub fn get_block(&self, hash: &BlockHash) -> Option<&Block> {
        self.blocks.get(hash)
    }
    
    /// Get the canonical block at a given height
    pub fn get_block_at_height(&self, height: BlockNumber) -> Option<&Block> {
        self.canonical_chain
            .get(&height)
            .and_then(|hash| self.blocks.get(hash))
    }
    
    /// Get the current head block
    pub fn get_head(&self) -> Option<&Block> {
        self.head.and_then(|hash| self.blocks.get(&hash))
    }
    
    /// Get the current chain height
    pub fn height(&self) -> BlockNumber {
        self.stats.chain_height
    }
    
    /// Insert a new block into the chain
    ///
    /// This handles:
    /// 1. Validation
    /// 2. Orphan detection
    /// 3. Fork choice
    /// 4. Reorgs if necessary
    pub fn insert_block(&mut self, block: Block) -> Result<BlockInsertResult, BuilderError> {
        // Check if we already have this block
        if self.blocks.contains_key(&block.hash) {
            return Ok(BlockInsertResult::AlreadyExists);
        }
        
        // Check if parent exists
        if !self.blocks.contains_key(&block.parent_hash) {
            // Parent not found - this is an orphan
            self.orphan_blocks.insert(block.hash, block.clone());
            self.stats.orphan_count += 1;
            return Ok(BlockInsertResult::Orphaned);
        }
        
        // Validate block
        self.validate_block(&block)?;
        
        // Insert the block
        let block_hash = block.hash;
        let block_number = block.number;
        
        // Track parent-child relationship
        self.children
            .entry(block.parent_hash)
            .or_insert_with(HashSet::new)
            .insert(block_hash);
        
        self.blocks.insert(block_hash, block);
        self.stats.total_blocks += 1;
        
        // Determine if this block becomes the new head
        let should_reorg = self.should_switch_to(&block_hash);
        
        if should_reorg {
            self.apply_fork_choice(block_hash)?;
        }
        
        // Process any orphans that were waiting for this block
        self.process_orphans(block_hash)?;
        
        if should_reorg {
            Ok(BlockInsertResult::NewHead { 
                reorg_depth: 0, // Will be set by apply_fork_choice
            })
        } else {
            Ok(BlockInsertResult::Inserted {
                is_canonical: self.canonical_chain.get(&block_number) == Some(&block_hash),
            })
        }
    }
    
    /// Validate a block before insertion
    fn validate_block(&self, block: &Block) -> Result<(), BuilderError> {
        // Check parent exists
        let parent = self.blocks.get(&block.parent_hash)
            .ok_or(BuilderError::Custom("Parent block not found".into()))?;
        
        // Check block number is parent + 1
        if block.number != parent.number + 1 {
            return Err(BuilderError::Custom(format!(
                "Invalid block number: expected {}, got {}",
                parent.number + 1,
                block.number
            )));
        }
        
        // Check timestamp is not before parent
        if block.timestamp < parent.timestamp {
            return Err(BuilderError::Custom(
                "Block timestamp before parent".into()
            ));
        }
        
        // Check gas used doesn't exceed limit
        if block.gas_used > block.gas_limit {
            return Err(BuilderError::Custom(
                "Gas used exceeds gas limit".into()
            ));
        }
        
        Ok(())
    }
    
    /// Determine if we should switch to a new fork
    fn should_switch_to(&self, block_hash: &BlockHash) -> bool {
        let block = match self.blocks.get(block_hash) {
            Some(b) => b,
            None => return false,
        };
        
        let current_head = match self.get_head() {
            Some(h) => h,
            None => return true, // No current head, accept any block
        };
        
        match self.fork_choice_rule {
            ForkChoiceRule::LongestChain => {
                block.number > current_head.number
            }
            ForkChoiceRule::HeaviestChain => {
                block.total_difficulty > current_head.total_difficulty
            }
            ForkChoiceRule::LatestJustified => {
                // Simplified: just use longest chain
                // Real implementation would check finality
                block.number > current_head.number
            }
        }
    }
    
    /// Apply fork choice and potentially reorg
    fn apply_fork_choice(&mut self, new_head: BlockHash) -> Result<(), BuilderError> {
        let new_block = self.blocks.get(&new_head)
            .ok_or(BuilderError::Custom("Block not found".into()))?
            .clone();
        
        // Find common ancestor
        let (common_ancestor, reorg_depth) = self.find_common_ancestor(&new_head)?;
        
        // Check reorg depth limit
        if reorg_depth > self.max_reorg_depth {
            return Err(BuilderError::Custom(format!(
                "Reorg too deep: {} blocks (max {})",
                reorg_depth,
                self.max_reorg_depth
            )));
        }
        
        if reorg_depth > 0 {
            self.stats.reorg_count += 1;
            self.stats.max_reorg_depth = self.stats.max_reorg_depth.max(reorg_depth);
        }
        
        // Rebuild canonical chain from common ancestor
        self.rebuild_canonical_chain(&new_head, &common_ancestor)?;
        
        // Update head
        self.head = Some(new_head);
        self.stats.chain_height = new_block.number;
        
        Ok(())
    }
    
    /// Find the common ancestor between new block and current head
    fn find_common_ancestor(&self, new_head: &BlockHash) -> Result<(BlockHash, u64), BuilderError> {
        let current_head = match self.head {
            Some(h) => h,
            None => return Ok((*new_head, 0)),
        };
        
        // Collect ancestors of both chains
        let mut new_ancestors: HashSet<BlockHash> = HashSet::new();
        let mut cursor = *new_head;
        
        while let Some(block) = self.blocks.get(&cursor) {
            new_ancestors.insert(cursor);
            if block.is_genesis() {
                break;
            }
            cursor = block.parent_hash;
        }
        
        // Walk back from current head to find common ancestor
        cursor = current_head;
        let mut depth = 0;
        
        while let Some(block) = self.blocks.get(&cursor) {
            if new_ancestors.contains(&cursor) {
                return Ok((cursor, depth));
            }
            if block.is_genesis() {
                break;
            }
            cursor = block.parent_hash;
            depth += 1;
        }
        
        // Genesis is always common ancestor
        let genesis_hash = self.canonical_chain.get(&0)
            .ok_or(BuilderError::Custom("No genesis block".into()))?;
        
        Ok((*genesis_hash, self.stats.chain_height))
    }
    
    /// Rebuild the canonical chain from common ancestor to new head
    fn rebuild_canonical_chain(
        &mut self, 
        new_head: &BlockHash,
        common_ancestor: &BlockHash
    ) -> Result<(), BuilderError> {
        // Collect blocks from new head back to common ancestor
        let mut new_chain: VecDeque<BlockHash> = VecDeque::new();
        let mut cursor = *new_head;
        
        while cursor != *common_ancestor {
            new_chain.push_front(cursor);
            let block = self.blocks.get(&cursor)
                .ok_or(BuilderError::Custom("Block not found during reorg".into()))?;
            cursor = block.parent_hash;
        }
        
        // Remove old canonical blocks above common ancestor
        let ancestor_block = self.blocks.get(common_ancestor)
            .ok_or(BuilderError::Custom("Ancestor not found".into()))?;
        let ancestor_height = ancestor_block.number;
        
        // Remove entries above ancestor
        let heights_to_remove: Vec<_> = self.canonical_chain
            .keys()
            .filter(|&&h| h > ancestor_height)
            .copied()
            .collect();
        
        for height in heights_to_remove {
            self.canonical_chain.remove(&height);
        }
        
        // Add new canonical blocks
        for hash in new_chain {
            let block = self.blocks.get(&hash)
                .ok_or(BuilderError::Custom("Block not found".into()))?;
            self.canonical_chain.insert(block.number, hash);
        }
        
        Ok(())
    }
    
    /// Process orphan blocks that might now have their parent
    fn process_orphans(&mut self, parent_hash: BlockHash) -> Result<(), BuilderError> {
        // Collect orphans that have this block as parent
        let orphans: Vec<Block> = self.orphan_blocks
            .iter()
            .filter(|(_, block)| block.parent_hash == parent_hash)
            .map(|(_, block)| block.clone())
            .collect();
        
        for orphan in orphans {
            let hash = orphan.hash;
            self.orphan_blocks.remove(&hash);
            self.stats.orphan_count = self.stats.orphan_count.saturating_sub(1);
            
            // Try to insert the orphan (it might still fail validation)
            let _ = self.insert_block(orphan);
        }
        
        Ok(())
    }
    
    /// Get chain statistics
    pub fn stats(&self) -> &ChainStats {
        &self.stats
    }
    
    /// Get all blocks at a given height (including non-canonical)
    pub fn get_blocks_at_height(&self, height: BlockNumber) -> Vec<&Block> {
        self.blocks
            .values()
            .filter(|block| block.number == height)
            .collect()
    }
    
    /// Detect all active forks
    pub fn detect_forks(&self) -> Vec<Fork> {
        let mut forks = Vec::new();
        
        // Find all blocks that have siblings (same parent, different hash)
        for (parent_hash, children) in &self.children {
            if children.len() > 1 {
                // This parent has multiple children = potential forks
                for child_hash in children {
                    if let Some(block) = self.blocks.get(child_hash) {
                        // Walk to the tip of this branch
                        let (tip, length) = self.find_branch_tip(child_hash);
                        
                        if let Some(tip_block) = self.blocks.get(&tip) {
                            forks.push(Fork {
                                fork_point: *parent_hash,
                                tip,
                                length,
                                total_difficulty: tip_block.total_difficulty,
                            });
                        }
                    }
                }
            }
        }
        
        forks
    }
    
    /// Find the tip of a branch starting from a block
    fn find_branch_tip(&self, start: &BlockHash) -> (BlockHash, u64) {
        let mut cursor = *start;
        let mut length = 1;
        
        loop {
            match self.children.get(&cursor) {
                Some(kids) if !kids.is_empty() => {
                    // Follow the first child (arbitrary choice for now)
                    cursor = *kids.iter().next().unwrap();
                    length += 1;
                }
                _ => break,
            }
        }
        
        (cursor, length)
    }
    
    /// Get the last N blocks from the canonical chain
    pub fn get_recent_blocks(&self, n: usize) -> Vec<&Block> {
        let height = self.stats.chain_height;
        let start = height.saturating_sub(n as u64 - 1);
        
        (start..=height)
            .filter_map(|h| self.get_block_at_height(h))
            .collect()
    }
    
    /// Check if a block is on the canonical chain
    pub fn is_canonical(&self, hash: &BlockHash) -> bool {
        if let Some(block) = self.blocks.get(hash) {
            self.canonical_chain.get(&block.number) == Some(hash)
        } else {
            false
        }
    }
    
    /// Get ancestors of a block up to a given depth
    pub fn get_ancestors(&self, hash: &BlockHash, depth: usize) -> Vec<&Block> {
        let mut ancestors = Vec::new();
        let mut cursor = *hash;
        
        for _ in 0..depth {
            if let Some(block) = self.blocks.get(&cursor) {
                if block.is_genesis() {
                    ancestors.push(block);
                    break;
                }
                ancestors.push(block);
                cursor = block.parent_hash;
            } else {
                break;
            }
        }
        
        ancestors
    }
}

/// Result of inserting a block
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockInsertResult {
    /// Block was inserted as new head, possibly with reorg
    NewHead { reorg_depth: u64 },
    
    /// Block was inserted but is not the new head
    Inserted { is_canonical: bool },
    
    /// Block already exists in the chain
    AlreadyExists,
    
    /// Block's parent is missing, stored as orphan
    Orphaned,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_block(parent: &Block, number: BlockNumber) -> Block {
        let mut block = Block {
            hash: [0u8; 32],
            parent_hash: parent.hash,
            number,
            state_root: [number as u8; 32],
            timestamp: parent.timestamp + 12,
            coinbase: [0u8; 20],
            total_difficulty: parent.total_difficulty + 1,
            gas_used: 1_000_000,
            gas_limit: 30_000_000,
            base_fee: 1_000_000_000,
            transaction_hashes: vec![],
            extra_data: vec![],
        };
        block.hash = block.compute_hash();
        block
    }
    
    #[test]
    fn test_genesis_creation() {
        let chain = ChainState::new(ForkChoiceRule::LongestChain);
        
        assert_eq!(chain.height(), 0);
        
        let head = chain.get_head().expect("Should have head");
        assert!(head.is_genesis());
    }
    
    #[test]
    fn test_linear_chain() {
        let mut chain = ChainState::new(ForkChoiceRule::LongestChain);
        
        let genesis = chain.get_head().unwrap().clone();
        
        // Build a linear chain of 5 blocks
        let mut parent = genesis;
        for i in 1..=5 {
            let block = create_test_block(&parent, i);
            let result = chain.insert_block(block.clone()).unwrap();
            
            assert!(matches!(result, BlockInsertResult::NewHead { .. }));
            assert_eq!(chain.height(), i);
            
            parent = block;
        }
        
        assert_eq!(chain.stats().total_blocks, 6); // genesis + 5
    }
    
    #[test]
    fn test_fork_detection() {
        let mut chain = ChainState::new(ForkChoiceRule::LongestChain);
        
        let genesis = chain.get_head().unwrap().clone();
        
        // Block 1
        let block1 = create_test_block(&genesis, 1);
        chain.insert_block(block1.clone()).unwrap();
        
        // Create two competing blocks at height 2
        let block2a = create_test_block(&block1, 2);
        chain.insert_block(block2a.clone()).unwrap();
        
        // Competing block with different hash
        let mut block2b = create_test_block(&block1, 2);
        block2b.timestamp += 1; // Different timestamp = different hash
        block2b.hash = block2b.compute_hash();
        chain.insert_block(block2b.clone()).unwrap();
        
        // Both blocks should exist
        let blocks_at_2 = chain.get_blocks_at_height(2);
        assert_eq!(blocks_at_2.len(), 2);
    }
    
    #[test]
    fn test_reorg() {
        let mut chain = ChainState::new(ForkChoiceRule::LongestChain);
        
        let genesis = chain.get_head().unwrap().clone();
        
        // Build chain: genesis -> 1 -> 2
        let block1 = create_test_block(&genesis, 1);
        chain.insert_block(block1.clone()).unwrap();
        
        let block2a = create_test_block(&block1, 2);
        chain.insert_block(block2a.clone()).unwrap();
        
        assert_eq!(chain.height(), 2);
        assert!(chain.is_canonical(&block2a.hash));
        
        // Create competing longer chain: genesis -> 1 -> 2b -> 3b
        let mut block2b = create_test_block(&block1, 2);
        block2b.timestamp += 1;
        block2b.hash = block2b.compute_hash();
        chain.insert_block(block2b.clone()).unwrap();
        
        let block3b = create_test_block(&block2b, 3);
        chain.insert_block(block3b.clone()).unwrap();
        
        // Should have reorged to the longer chain
        assert_eq!(chain.height(), 3);
        assert!(chain.is_canonical(&block3b.hash));
        assert!(chain.is_canonical(&block2b.hash));
        assert!(!chain.is_canonical(&block2a.hash)); // Old block no longer canonical
        
        assert!(chain.stats().reorg_count > 0);
    }
    
    #[test]
    fn test_orphan_handling() {
        let mut chain = ChainState::new(ForkChoiceRule::LongestChain);
        
        let genesis = chain.get_head().unwrap().clone();
        
        // Create blocks 1, 2, 3
        let block1 = create_test_block(&genesis, 1);
        let block2 = create_test_block(&block1, 2);
        let block3 = create_test_block(&block2, 3);
        
        // Insert block 3 first (orphan - parent not found)
        let result = chain.insert_block(block3.clone()).unwrap();
        assert_eq!(result, BlockInsertResult::Orphaned);
        
        // Insert block 1
        chain.insert_block(block1.clone()).unwrap();
        
        // Insert block 2 - should also process orphaned block 3
        chain.insert_block(block2.clone()).unwrap();
        
        // Block 3 should now be in chain
        assert_eq!(chain.height(), 3);
    }
}
