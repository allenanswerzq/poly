//! # Fraud Proofs
//!
//! Optimistic rollup fraud proof mechanism

use eth_primitives::{H256, Address};
use crate::transaction::L2Transaction;
use crate::batch::{Batch, BatchHeader};
use crate::error::{RollupError, Result};

/// Fraud proof for disputing an invalid batch
#[derive(Debug, Clone)]
pub struct FraudProof {
    /// Batch being challenged
    pub batch_number: u64,
    /// Transaction index in batch
    pub tx_index: usize,
    /// Pre-state proof (accounts involved)
    pub pre_state: Vec<AccountProof>,
    /// Transaction that was executed incorrectly
    pub transaction: L2Transaction,
    /// Claimed post-state root (from batch header)
    pub claimed_post_root: H256,
    /// Correct post-state root (computed by prover)
    pub correct_post_root: H256,
}

/// Proof of account state
#[derive(Debug, Clone)]
pub struct AccountProof {
    /// Account address
    pub address: Address,
    /// Account balance
    pub balance: u64,
    /// Account nonce
    pub nonce: u64,
    /// Merkle proof nodes
    pub proof_nodes: Vec<Vec<u8>>,
}

impl FraudProof {
    /// Create fraud proof for a batch
    pub fn create(
        batch: &Batch,
        tx_index: usize,
        pre_state: Vec<AccountProof>,
        correct_post_root: H256,
    ) -> Self {
        FraudProof {
            batch_number: batch.header.batch_number,
            tx_index,
            pre_state,
            transaction: batch.transactions[tx_index].clone(),
            claimed_post_root: batch.header.post_state_root,
            correct_post_root,
        }
    }

    /// Verify the fraud proof is valid
    pub fn verify(&self) -> bool {
        // In a real implementation:
        // 1. Verify pre-state proof against batch.pre_state_root
        // 2. Re-execute transaction
        // 3. Verify post-state doesn't match claimed

        // Simplified: just check roots differ
        self.claimed_post_root != self.correct_post_root
    }
}

/// Challenge status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeStatus {
    /// Challenge is pending
    Pending,
    /// Challenge was proven (fraud found)
    Proven,
    /// Challenge was rejected (no fraud)
    Rejected,
    /// Challenge period expired
    Expired,
}

/// Challenge record
#[derive(Debug, Clone)]
pub struct Challenge {
    /// Challenge ID
    pub id: u64,
    /// Batch being challenged
    pub batch_number: u64,
    /// Challenger address
    pub challenger: Address,
    /// Challenge bond amount
    pub bond: u64,
    /// Challenge status
    pub status: ChallengeStatus,
    /// Fraud proof
    pub proof: Option<FraudProof>,
    /// Timestamp when challenge was created
    pub created_at: u64,
    /// Timestamp when challenge was resolved
    pub resolved_at: Option<u64>,
}

impl Challenge {
    /// Create new challenge
    pub fn new(
        id: u64,
        batch_number: u64,
        challenger: Address,
        bond: u64,
    ) -> Self {
        Challenge {
            id,
            batch_number,
            challenger,
            bond,
            status: ChallengeStatus::Pending,
            proof: None,
            created_at: current_timestamp(),
            resolved_at: None,
        }
    }

    /// Submit fraud proof for this challenge
    pub fn submit_proof(&mut self, proof: FraudProof) {
        self.proof = Some(proof);
    }

    /// Resolve the challenge
    pub fn resolve(&mut self, proven: bool) {
        self.status = if proven {
            ChallengeStatus::Proven
        } else {
            ChallengeStatus::Rejected
        };
        self.resolved_at = Some(current_timestamp());
    }
}

/// L1 Rollup Contract (simulated)
pub struct L1RollupContract {
    /// Challenge period in seconds
    pub challenge_period: u64,
    /// Minimum challenge bond
    pub min_bond: u64,
    /// Committed batches
    pub batches: Vec<BatchCommitment>,
    /// Active challenges
    pub challenges: Vec<Challenge>,
    /// Next challenge ID
    pub next_challenge_id: u64,
    /// Finalized batch number
    pub finalized_batch: u64,
}

/// Batch commitment on L1
#[derive(Debug, Clone)]
pub struct BatchCommitment {
    pub header: BatchHeader,
    pub commit_timestamp: u64,
    pub is_finalized: bool,
}

impl L1RollupContract {
    /// Create new contract
    pub fn new(challenge_period: u64, min_bond: u64) -> Self {
        L1RollupContract {
            challenge_period,
            min_bond,
            batches: Vec::new(),
            challenges: Vec::new(),
            next_challenge_id: 1,
            finalized_batch: 0,
        }
    }

    /// Commit batch (called by sequencer)
    pub fn commit_batch(&mut self, header: BatchHeader) {
        self.batches.push(BatchCommitment {
            header,
            commit_timestamp: current_timestamp(),
            is_finalized: false,
        });
    }

    /// Create challenge for a batch
    pub fn create_challenge(
        &mut self,
        batch_number: u64,
        challenger: Address,
        bond: u64,
    ) -> Result<u64> {
        if bond < self.min_bond {
            return Err(RollupError::InvalidProof);
        }

        // Check batch exists and is challengeable
        let batch = self.batches.iter()
            .find(|b| b.header.batch_number == batch_number)
            .ok_or_else(|| RollupError::SequencerError("Batch not found".to_string()))?;

        if batch.is_finalized {
            return Err(RollupError::SequencerError("Batch already finalized".to_string()));
        }

        let challenge_id = self.next_challenge_id;
        self.next_challenge_id += 1;

        let challenge = Challenge::new(
            challenge_id,
            batch_number,
            challenger,
            bond,
        );

        self.challenges.push(challenge);

        Ok(challenge_id)
    }

    /// Submit fraud proof
    pub fn submit_fraud_proof(
        &mut self,
        challenge_id: u64,
        proof: FraudProof,
    ) -> Result<bool> {
        let challenge = self.challenges.iter_mut()
            .find(|c| c.id == challenge_id)
            .ok_or_else(|| RollupError::SequencerError("Challenge not found".to_string()))?;

        if challenge.status != ChallengeStatus::Pending {
            return Err(RollupError::SequencerError("Challenge already resolved".to_string()));
        }

        challenge.submit_proof(proof.clone());

        // Verify proof
        let is_valid = proof.verify();
        challenge.resolve(is_valid);

        if is_valid {
            // Slash sequencer, revert batch
            self.revert_batch(proof.batch_number);
        }

        Ok(is_valid)
    }

    /// Revert a batch and all subsequent batches
    fn revert_batch(&mut self, batch_number: u64) {
        // Mark all batches from this point as invalid
        for batch in &mut self.batches {
            if batch.header.batch_number >= batch_number {
                batch.is_finalized = false;
            }
        }
    }

    /// Finalize batches past challenge period
    pub fn finalize_batches(&mut self) {
        let now = current_timestamp();

        for batch in &mut self.batches {
            if batch.is_finalized {
                continue;
            }

            // Check if challenge period passed
            if now >= batch.commit_timestamp + self.challenge_period {
                // Check no pending challenges
                let has_challenge = self.challenges.iter()
                    .any(|c| c.batch_number == batch.header.batch_number
                         && c.status == ChallengeStatus::Pending);

                if !has_challenge {
                    batch.is_finalized = true;
                    if batch.header.batch_number > self.finalized_batch {
                        self.finalized_batch = batch.header.batch_number;
                    }
                }
            }
        }
    }

    /// Get finalized batch number
    pub fn get_finalized_batch(&self) -> u64 {
        self.finalized_batch
    }
}

fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fraud_proof_verification() {
        let claimed = H256::new([1u8; 32]);
        let correct = H256::new([2u8; 32]);

        let from = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let to = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

        let proof = FraudProof {
            batch_number: 1,
            tx_index: 0,
            pre_state: vec![],
            transaction: L2Transaction::transfer(from, to, 1000, 0),
            claimed_post_root: claimed,
            correct_post_root: correct,
        };

        assert!(proof.verify());
    }

    #[test]
    fn test_l1_contract() {
        let mut contract = L1RollupContract::new(
            7 * 24 * 60 * 60, // 7 days
            1_000_000_000, // 1 ETH bond
        );

        // Commit batch
        let header = BatchHeader {
            batch_number: 1,
            parent_hash: H256::default(),
            pre_state_root: H256::default(),
            post_state_root: H256::new([1u8; 32]),
            tx_root: H256::default(),
            tx_count: 10,
            timestamp: current_timestamp(),
            sequencer: [0u8; 20],
        };

        contract.commit_batch(header);
        assert_eq!(contract.batches.len(), 1);
        assert!(!contract.batches[0].is_finalized);
    }

    #[test]
    fn test_challenge() {
        let mut contract = L1RollupContract::new(
            60, // 60 seconds for testing
            1000,
        );

        let challenger = Address::from_hex("0x3333333333333333333333333333333333333333").unwrap();

        // Commit batch
        let header = BatchHeader {
            batch_number: 1,
            parent_hash: H256::default(),
            pre_state_root: H256::default(),
            post_state_root: H256::new([1u8; 32]),
            tx_root: H256::default(),
            tx_count: 1,
            timestamp: current_timestamp(),
            sequencer: [0u8; 20],
        };
        contract.commit_batch(header);

        // Create challenge
        let challenge_id = contract.create_challenge(1, challenger, 10000).unwrap();
        assert_eq!(challenge_id, 1);
        assert_eq!(contract.challenges.len(), 1);
    }
}
