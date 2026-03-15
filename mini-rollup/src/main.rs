//! # Mini Rollup Demo
//!
//! Demonstrates a minimal L2 rollup sequencer

use mini_rollup::{
    L2Transaction, Sequencer, SequencerConfig,
    L1RollupContract, FraudProof, BatchHeader,
};
use eth_primitives::{Address, H256};

fn main() {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                   Mini Rollup Demo                         ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Demo 1: Create and run sequencer
    demo_sequencer();

    // Demo 2: L1 contract simulation
    demo_l1_contract();

    // Demo 3: Fraud proof
    demo_fraud_proof();

    println!("\n✅ All demos completed successfully!");
}

fn demo_sequencer() {
    println!("📦 Demo 1: L2 Sequencer");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Create addresses
    let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
    let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
    let charlie = Address::from_hex("0x3333333333333333333333333333333333333333").unwrap();

    // Create sequencer config
    let config = SequencerConfig {
        sequencer_address: Address::from_hex("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").unwrap(),
        max_batch_size: 5,
        batch_interval: 5,
        challenge_period: 7 * 24 * 60 * 60, // 7 days
    };

    println!("  Config: max_batch_size={}, batch_interval={}s",
             config.max_batch_size, config.batch_interval);

    // Create sequencer
    let mut sequencer = Sequencer::new(config);

    // Fund accounts (enough for gas + value)
    sequencer.state_mut().set_balance(alice, 100_000_000_000_000_000);
    sequencer.state_mut().set_balance(bob, 50_000_000_000_000_000);

    let initial_root = sequencer.state_root();
    println!("  Initial state root: {}", initial_root);
    println!("  Alice balance: {}", sequencer.state().get_balance(&alice));
    println!("  Bob balance: {}", sequencer.state().get_balance(&bob));

    // Submit transactions
    println!("\n  Submitting transactions...");

    let tx1 = L2Transaction::transfer(alice, bob, 10000, 0);
    let tx2 = L2Transaction::transfer(bob, charlie, 5000, 0);
    let tx3 = L2Transaction::transfer(alice, charlie, 3000, 1);

    sequencer.submit_transaction(tx1.clone()).unwrap();
    sequencer.submit_transaction(tx2.clone()).unwrap();
    sequencer.submit_transaction(tx3.clone()).unwrap();

    println!("    ✓ Alice → Bob: 10000");
    println!("    ✓ Bob → Charlie: 5000");
    println!("    ✓ Alice → Charlie: 3000");

    println!("  Pending transactions: {}", sequencer.pending_count());

    // Seal batch
    println!("\n  Sealing batch...");
    let mut batch = sequencer.seal_batch().unwrap();

    println!("    Batch #{}", batch.header.batch_number);
    println!("    Transactions: {}", batch.transactions.len());
    println!("    Pre-state root: {}", batch.header.pre_state_root);
    println!("    Post-state root: {}", batch.header.post_state_root);

    // Check final balances
    println!("\n  Final balances:");
    println!("    Alice: {}", sequencer.state().get_balance(&alice));
    println!("    Bob: {}", sequencer.state().get_balance(&bob));
    println!("    Charlie: {}", sequencer.state().get_balance(&charlie));

    // Compress batch for L1
    let compressed = batch.compress();
    let original_size = batch.transactions.len() * 100; // rough estimate
    println!("\n  Batch compression:");
    println!("    Original (estimate): {} bytes", original_size);
    println!("    Compressed: {} bytes", compressed.len());

    println!();
}

fn demo_l1_contract() {
    println!("⛓️  Demo 2: L1 Rollup Contract Simulation");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Create L1 contract with 1 week challenge period
    let challenge_period = 7 * 24 * 60 * 60; // 7 days
    let min_bond = 1_000_000_000; // 1 ETH in wei (scaled down)

    let mut contract = L1RollupContract::new(challenge_period, min_bond);

    println!("  Challenge period: {} days", challenge_period / 86400);
    println!("  Minimum bond: {} wei", min_bond);

    // Simulate committing batches
    println!("\n  Committing batches to L1...");

    for i in 1..=3 {
        let header = BatchHeader {
            batch_number: i,
            parent_hash: H256::new([i as u8; 32]),
            pre_state_root: H256::new([i as u8; 32]),
            post_state_root: H256::new([(i + 1) as u8; 32]),
            tx_root: H256::new([i as u8; 32]),
            tx_count: 10,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            sequencer: [0xFFu8; 20],
        };

        contract.commit_batch(header);
        println!("    ✓ Batch #{} committed", i);
    }

    println!("\n  Committed batches: {}", contract.batches.len());
    println!("  Finalized batch: {}", contract.get_finalized_batch());

    // Note about finalization
    println!("\n  ℹ️  Batches finalize after challenge period (7 days)");
    println!("     Validators can submit fraud proofs during this window");

    println!();
}

fn demo_fraud_proof() {
    println!("🔍 Demo 3: Fraud Proof Mechanism");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Simulate incorrect state transition
    let claimed_root = H256::new([0xAA; 32]);
    let correct_root = H256::new([0xBB; 32]);

    let from = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
    let to = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

    println!("  Scenario: Sequencer claims incorrect post-state root");
    println!("    Claimed root: {}", claimed_root);
    println!("    Correct root: {}", correct_root);

    // Create fraud proof
    let proof = FraudProof {
        batch_number: 5,
        tx_index: 3,
        pre_state: vec![],
        transaction: L2Transaction::transfer(from, to, 100000, 0),
        claimed_post_root: claimed_root,
        correct_post_root: correct_root,
    };

    println!("\n  Creating fraud proof for batch #{}, tx #{}",
             proof.batch_number, proof.tx_index);

    // Verify proof
    let is_valid = proof.verify();

    if is_valid {
        println!("\n  ✅ Fraud proof VERIFIED!");
        println!("     The sequencer committed an invalid state transition");
        println!("     Actions:");
        println!("       • Batch #{} and all subsequent batches reverted", proof.batch_number);
        println!("       • Sequencer's stake slashed");
        println!("       • Challenger receives reward");
    } else {
        println!("\n  ❌ Fraud proof INVALID");
        println!("     The batch is correct");
    }

    println!();

    // Explain the challenge flow
    println!("  📋 Challenge Flow:");
    println!("  ┌─────────────────────────────────────────────────────────┐");
    println!("  │  1. Sequencer commits batch to L1                       │");
    println!("  │  2. 7-day challenge window opens                        │");
    println!("  │  3. Anyone can submit fraud proof with bond             │");
    println!("  │  4. If fraud proven: batch reverted, sequencer slashed  │");
    println!("  │  5. If no fraud: batch finalized after 7 days           │");
    println!("  └─────────────────────────────────────────────────────────┘");

    println!();
}
