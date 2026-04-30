//! # Block Builder Demo
//!
//! Demonstrates block construction, MEV concepts, and full PBS simulation

use block_builder::{
    BlockBuilder, BuilderConfig, Mempool, MempoolConfig,
    PendingTransaction, TransactionPriority,
    MevBundle, MevExtractor, Opportunity, Strategy,
    // PBS modules
    Relay, RelayConfig,
    Slot, SlotClock,
};
use eth_primitives::Address;
use std::time::Duration;

fn main() {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║          Block Builder Demo - Mini ETH Simulation          ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Demo 1: Mempool and transaction ordering
    demo_mempool();

    // Demo 2: Block building
    demo_block_building();

    // Demo 3: MEV concepts
    demo_mev();

    // Demo 4: Full builder simulation
    demo_builder_simulation();

    // Demo 5: Full PBS pipeline (new!)
    demo_pbs_pipeline();

    // Demo 6: Chain and reorgs (new!)
    demo_chain_reorgs();

    // Demo 7: Network simulation (new!)
    demo_network();

    println!("\n✅ All demos completed successfully!");
}

fn demo_mempool() {
    println!("📝 Demo 1: Mempool & Transaction Ordering");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
    let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

    let mut mempool = Mempool::new(MempoolConfig {
        max_size: 100,
        max_per_sender: 10,
        tx_lifetime: 3600,
    });

    println!("  Adding transactions with different priority fees...\n");

    // Add transactions with varying priority fees
    let fees = [(5, "Low"), (15, "Medium"), (25, "High"), (10, "Normal")];

    for (i, (fee, label)) in fees.iter().enumerate() {
        let tx = PendingTransaction::transfer(
            alice, bob, 1000, i as u64,
            100_000_000_000, // 100 gwei max fee
            *fee as u64 * 1_000_000_000, // priority fee in gwei
        );
        println!("    Tx #{}: {} priority fee ({} gwei)", i, label, fee);
        mempool.add(tx).unwrap();
    }

    println!("\n  Mempool stats:");
    let stats = mempool.stats();
    println!("    Total transactions: {}", stats.total_txs);
    println!("    Total pending gas: {}", stats.total_gas);

    // Get ordered transactions
    let ordered = mempool.get_ordered(1_000_000, 50_000_000_000);

    println!("\n  Ordered by priority (highest first):");
    for (i, tx) in ordered.iter().enumerate() {
        let fee_gwei = tx.max_priority_fee / 1_000_000_000;
        println!("    {}. Priority fee: {} gwei", i + 1, fee_gwei);
    }

    println!();
}

fn demo_block_building() {
    println!("🏗️  Demo 2: Block Building");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
    let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
    let charlie = Address::from_hex("0x3333333333333333333333333333333333333333").unwrap();

    let config = BuilderConfig {
        gas_limit: 300_000, // Limited for demo
        base_fee: 30_000_000_000, // 30 gwei
        min_priority_fee: 1_000_000_000, // 1 gwei minimum
        builder_fee_percent: 10,
    };

    let mut builder = BlockBuilder::new(config);

    println!("  Builder config:");
    println!("    Gas limit: {}", builder.config().gas_limit);
    println!("    Base fee: {} gwei", builder.base_fee() / 1_000_000_000);

    // Add various transactions
    println!("\n  Submitting transactions to mempool...");

    let txs = [
        (alice, bob, 5, "Alice → Bob (5 gwei tip)"),
        (bob, charlie, 10, "Bob → Charlie (10 gwei tip)"),
        (alice, charlie, 2, "Alice → Charlie (2 gwei tip)"),
        (charlie, bob, 15, "Charlie → Bob (15 gwei tip)"),
    ];

    for (i, (from, to, tip_gwei, desc)) in txs.iter().enumerate() {
        let tx = PendingTransaction::transfer(
            *from, *to, 1000, i as u64,
            100_000_000_000,
            *tip_gwei as u64 * 1_000_000_000,
        );
        builder.mempool_mut().add(tx).unwrap();
        println!("    ✓ {}", desc);
    }

    // Simulate building
    let (tx_count, gas, profit) = builder.simulate_build();
    println!("\n  Build simulation:");
    println!("    Transactions: {}", tx_count);
    println!("    Gas used: {} / {}", gas, builder.config().gas_limit);
    println!("    Expected profit: {} gwei", profit / 1_000_000_000);

    // Build block
    println!("\n  Building block...");
    let block = builder.build_block().unwrap();

    println!("    Block #{}", block.number);
    println!("    Hash: {}", block.hash());
    println!("    Transactions: {}", block.transactions.len());
    println!("    Gas used: {} / {}", block.gas_used, block.gas_limit);
    println!("    Builder profit: {} gwei", block.builder_profit / 1_000_000_000);

    println!();
}

fn demo_mev() {
    println!("💰 Demo 3: MEV (Maximal Extractable Value)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
    let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();

    println!("  MEV Opportunity Types:");
    println!("  ┌─────────────────────────────────────────────────────────┐");
    println!("  │  🔄 Arbitrage   - Profit from price differences         │");
    println!("  │  💧 Liquidation - Liquidate undercollateralized loans   │");
    println!("  │  🥪 Sandwich    - Front/backrun trades (controversial)  │");
    println!("  │  ⚡ JIT         - Just-in-time liquidity provision      │");
    println!("  │  🏃 Backrun     - Follow profitable transactions        │");
    println!("  └─────────────────────────────────────────────────────────┘");

    // Create an arbitrage opportunity
    println!("\n  Simulating MEV opportunities...");

    let arb_tx = PendingTransaction::new(
        alice,
        Some(bob), // DEX contract
        0,
        vec![0xaa, 0xbb, 0xcc], // Swap calldata
        0,
        150_000, // Higher gas for DEX interaction
        100_000_000_000,
        20_000_000_000, // 20 gwei tip
    );

    let opp = Opportunity::arbitrage(500_000_000_000, arb_tx); // 500 gwei profit

    println!("\n  📊 Arbitrage Opportunity:");
    println!("    Expected profit: {} gwei", opp.expected_profit / 1_000_000_000);
    println!("    Risk score: {}/100", opp.risk_score);

    let base_fee = 30_000_000_000; // 30 gwei
    let net = opp.net_profit(base_fee);
    println!("    Net profit (after gas): {} gwei", net / 1_000_000_000);
    println!("    Profitable: {}", if opp.is_profitable(base_fee) { "✅ Yes" } else { "❌ No" });

    // MEV strategies
    println!("\n  📋 MEV Strategies:");
    let strategies = [
        (Strategy::ArbitrageOnly, "Arbitrage Only"),
        (Strategy::EthicalMev, "Ethical MEV"),
        (Strategy::MaxExtraction, "Max Extraction"),
    ];

    for (strategy, name) in &strategies {
        let types = strategy.allowed_types();
        println!("    {} ({} types allowed):", name, types.len());
        for t in types {
            println!("      - {:?}", t);
        }
    }

    // Bundle creation
    println!("\n  Creating MEV bundle...");
    let extractor = MevExtractor::new(1_000_000_000); // 1 gwei min profit

    let bundle_tx = PendingTransaction::transfer(
        alice, bob, 0, 0,
        100_000_000_000,
        25_000_000_000,
    ).with_priority(TransactionPriority::High);

    let bundle = MevBundle::new(vec![bundle_tx], 100);

    println!("    Bundle ID: {}", bundle.id);
    println!("    Target block: {}", bundle.target_block);
    println!("    Total gas: {}", bundle.total_gas());
    println!("    Total tip: {} gwei", bundle.total_tip() / 1_000_000_000);

    println!();
}

fn demo_builder_simulation() {
    println!("🎮 Demo 4: Full Builder Simulation");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
    let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
    let searcher = Address::from_hex("0xABCDABCDABCDABCDABCDABCDABCDABCDABCDABCD").unwrap();

    let config = BuilderConfig {
        gas_limit: 1_000_000,
        base_fee: 25_000_000_000, // 25 gwei
        min_priority_fee: 2_000_000_000, // 2 gwei
        builder_fee_percent: 10,
    };

    let mut builder = BlockBuilder::new(config);

    // Simulate 3 blocks
    for block_num in 1..=3 {
        println!("\n  📦 Block #{}", block_num);
        println!("  ────────────────────────────");

        // Add regular transactions
        let num_regular = 5 + (block_num * 2);
        for i in 0..num_regular {
            let tip = (i + 5) as u64 * 1_000_000_000;
            let tx = PendingTransaction::transfer(
                alice, bob, 1000, i as u64, 100_000_000_000, tip
            );
            let _ = builder.mempool_mut().add(tx);
        }
        println!("    Regular txs in mempool: {}", builder.mempool().len());

        // Add MEV bundle (blocks 2 and 3)
        if block_num > 1 {
            let mev_tx = PendingTransaction::transfer(
                searcher, bob, 0, 0,
                100_000_000_000,
                50_000_000_000, // 50 gwei tip
            ).with_priority(TransactionPriority::High);

            let bundle = MevBundle::new(vec![mev_tx], 0);
            builder.submit_bundle(bundle).unwrap();
            println!("    MEV bundle submitted: 50 gwei tip");
        }

        // Build block
        let block = builder.build_block().unwrap();

        println!("    Built: {} txs, {} gas used",
                 block.transactions.len(),
                 block.gas_used);
        println!("    Builder profit: {} gwei",
                 block.builder_profit / 1_000_000_000);

        // Update base fee based on usage
        builder.update_base_fee(block.gas_used);
        println!("    New base fee: {} gwei",
                 builder.base_fee() / 1_000_000_000);
    }

    // Summary
    println!("\n  📋 PBS/MEV Summary:");
    println!("  ┌─────────────────────────────────────────────────────────┐");
    println!("  │  Proposer-Builder Separation (PBS):                     │");
    println!("  │  • Validators propose blocks but don't build them       │");
    println!("  │  • Builders compete to create most profitable blocks    │");
    println!("  │  • MEV is extracted by searchers and shared with builders│");
    println!("  │  • Block space becomes an auction marketplace           │");
    println!("  └─────────────────────────────────────────────────────────┘");

    println!();
}

fn demo_pbs_pipeline() {
    println!("🔗 Demo 5: Full PBS Pipeline");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("  The PBS pipeline shows how blocks flow in modern Ethereum:");
    println!("  Searchers → Builders → Relay → Proposer → Chain");
    println!();

    // Step 1: Create a Slot Clock
    let clock = SlotClock::new_simulation();

    // Simulate being in slot 100
    let current_slot = Slot::new(100);
    let epoch = current_slot.epoch();

    println!("  📅 Slot Timing:");
    println!("    Current slot: {}", current_slot);
    println!("    Current epoch: {}", epoch);
    println!("    Slot in epoch: {}/32", current_slot.slot_in_epoch());
    println!("    Epoch start slot: {}", epoch.start_slot());
    println!();

    // Step 2: Create a Relay
    use block_builder::relay::{Relay, RelayConfig, BuilderId, BuilderSubmission as RelaySubmission};

    let relay_config = RelayConfig::default();
    let mut relay = Relay::new(relay_config);
    println!("  🔄 Relay Created:");
    println!("    Min bid: {} gwei", relay.config().min_bid / 1_000_000_000);
    println!();

    // Step 3: Builders submit blocks
    println!("  🏗️  Builder Submissions:");

    let builders = [
        ("Flashbots", 50_000_000_000u64),   // 50 gwei
        ("BloXroute", 48_000_000_000u64),   // 48 gwei
        ("Titan", 45_000_000_000u64),       // 45 gwei
    ];

    for (i, (name, bid)) in builders.iter().enumerate() {
        // Create a simple builder block
        let builder_config = BuilderConfig {
            gas_limit: 1_000_000,
            base_fee: 25_000_000_000,
            min_priority_fee: 1_000_000_000,
            builder_fee_percent: 10,
        };
        let mut builder = BlockBuilder::new(builder_config);

        // Add some txs
        let alice = Address::from_hex("0x1111111111111111111111111111111111111111").unwrap();
        let bob = Address::from_hex("0x2222222222222222222222222222222222222222").unwrap();
        for j in 0..3 {
            let tx = PendingTransaction::transfer(alice, bob, 1000, j, 100_000_000_000, 10_000_000_000);
            let _ = builder.mempool_mut().add(tx);
        }

        let block = builder.build_block().unwrap();

        // Create a 32-byte builder ID from name
        let mut builder_id_bytes = [0u8; 32];
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(32);
        builder_id_bytes[..len].copy_from_slice(&name_bytes[..len]);
        builder_id_bytes[31] = i as u8; // Make each unique

        let builder_id = BuilderId::new(builder_id_bytes);
        let submission = RelaySubmission::new(builder_id, block, *bid);

        relay.submit_block(submission).ok();
        println!("    {} submitted block with bid: {} gwei", name, bid / 1_000_000_000);
    }

    // Step 4: Show auction status
    println!();
    println!("  🏆 Auction Results:");
    let all_bids = relay.get_all_bids(100);
    if !all_bids.is_empty() {
        for (builder_id, bid) in &all_bids {
            println!("    {} bid {} gwei", builder_id, bid / 1_000_000_000);
        }
    } else {
        println!("    (Bids are for current slot, showing submission flow)");
        println!("    Flashbots would win with highest bid (50 gwei)");
    }

    // Step 5: Proposer flow
    println!();
    println!("  👤 Proposer Actions:");
    println!("    1. Query relay for best header (blind)");
    println!("    2. Sign commitment to header");
    println!("    3. Receive full block from relay");
    println!("    4. Broadcast to network");

    // Step 6: Summary diagram
    println!();
    println!("  📊 Complete PBS Flow:");
    println!("  ┌──────────────────────────────────────────────────────────┐");
    println!("  │                                                          │");
    println!("  │   Searchers ──MEV bundles──▶ Builders                    │");
    println!("  │                                │                         │");
    println!("  │                          Submit blocks                   │");
    println!("  │                                │                         │");
    println!("  │                                ▼                         │");
    println!("  │                          ┌─────────┐                     │");
    println!("  │                          │  RELAY  │ ◀──Run auction      │");
    println!("  │                          └────┬────┘                     │");
    println!("  │                               │                          │");
    println!("  │                         Blind header                     │");
    println!("  │                               │                          │");
    println!("  │                               ▼                          │");
    println!("  │   Validators ◀────────── Proposer ──────▶ Network        │");
    println!("  │   (attestors)          (signs header)                    │");
    println!("  │                                                          │");
    println!("  └──────────────────────────────────────────────────────────┘");
    println!();

    // Use the clock variable
    let _ = clock.current_slot();
}

fn demo_chain_reorgs() {
    println!("🔀 Demo 6: Chain & Reorgs");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    use block_builder::chain::{Block, ChainState, ForkChoiceRule, BlockInsertResult};

    let mut chain = ChainState::new(ForkChoiceRule::LongestChain);
    println!("\n  📦 Chain initialized with genesis block");
    println!("    Height: {}", chain.height());

    // Build main chain
    println!("\n  Building main chain (blocks 1-5)...");
    let genesis = chain.get_head().unwrap().clone();
    let mut parent = genesis;

    for i in 1..=5 {
        let mut block = Block::genesis();
        block.parent_hash = parent.hash;
        block.number = i;
        block.timestamp = parent.timestamp + 12;
        block.total_difficulty = parent.total_difficulty + 1;
        block.extra_data = format!("Block {}", i).into_bytes();
        block.hash = block.compute_hash();

        chain.insert_block(block.clone()).unwrap();
        parent = block;
    }

    println!("    Current height: {}", chain.height());
    println!("    Total blocks: {}", chain.stats().total_blocks);

    // Create a fork at block 3
    println!("\n  🔱 Creating fork at block 3...");
    let block_2 = chain.get_block_at_height(2).unwrap().clone();

    // Fork block 3a (already exists as the canonical one)
    // Create 3b competing block
    let mut block_3b = Block::genesis();
    block_3b.parent_hash = block_2.hash;
    block_3b.number = 3;
    block_3b.timestamp = block_2.timestamp + 15; // Different timing
    block_3b.total_difficulty = block_2.total_difficulty + 2; // Slightly heavier
    block_3b.extra_data = b"Block 3b (fork)".to_vec();
    block_3b.hash = block_3b.compute_hash();

    chain.insert_block(block_3b.clone()).unwrap();
    println!("    Created competing block 3b");

    // Build longer fork
    let mut fork_parent = block_3b.clone();
    for i in 4..=7 {
        let mut block = Block::genesis();
        block.parent_hash = fork_parent.hash;
        block.number = i;
        block.timestamp = fork_parent.timestamp + 12;
        block.total_difficulty = fork_parent.total_difficulty + 1;
        block.extra_data = format!("Block {}b (fork)", i).into_bytes();
        block.hash = block.compute_hash();

        let result = chain.insert_block(block.clone()).unwrap();

        if matches!(result, BlockInsertResult::NewHead { .. }) {
            println!("    ⚡ Block {} caused reorg!", i);
        }

        fork_parent = block;
    }

    println!("\n  📊 After fork resolution:");
    println!("    Chain height: {}", chain.height());
    println!("    Reorg count: {}", chain.stats().reorg_count);

    // Show canonical chain
    println!("\n  📝 Canonical chain:");
    let recent = chain.get_recent_blocks(5);
    for block in recent {
        let extra = std::str::from_utf8(&block.extra_data).unwrap_or("...");
        println!("    Block {}: {}", block.number, extra);
    }

    println!();
}

fn demo_network() {
    println!("🌐 Demo 7: Network Simulation");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    use block_builder::network::{NetworkSimulator, NodeType, NetworkMessage};
    use block_builder::chain::Block;

    // Build a test network
    println!("\n  Building PBS network topology...");
    let mut network = NetworkSimulator::build_test_network(
        4,  // validators
        3,  // builders
        2,  // relays
    );

    // Get counts first (doesn't hold references)
    let num_validators = network.get_nodes_by_type(NodeType::Validator).len();
    let num_builders = network.get_nodes_by_type(NodeType::Builder).len();
    let num_relays = network.get_nodes_by_type(NodeType::Relay).len();

    println!("    Validators: {}", num_validators);
    println!("    Builders: {}", num_builders);
    println!("    Relays: {}", num_relays);

    // Get validator info for later use
    let validator_ids: Vec<_> = network.get_nodes_by_type(NodeType::Validator)
        .iter()
        .map(|v| v.id)
        .collect();
    let total_peers: usize = network.get_nodes_by_type(NodeType::Validator)
        .iter()
        .map(|v| v.peers.len())
        .sum();

    println!("    Total validator connections: {}", total_peers);

    // Get relay ID
    let relay_id = network.get_nodes_by_type(NodeType::Relay)[0].id;

    // Simulate block propagation
    println!("\n  📡 Simulating block propagation...");

    let block = Block::genesis();

    // Relay broadcasts new block
    let msg_ids = network.broadcast(relay_id, NetworkMessage::NewBlock(block));
    println!("    Relay broadcast to {} peers", msg_ids.len());
    println!("    Messages in flight: {}", network.pending_messages());

    // Advance time
    network.tick(Duration::from_millis(100));
    println!("\n  ⏱️  After 100ms:");
    println!("    Messages delivered: {}", network.stats().messages_delivered);
    println!("    Messages in flight: {}", network.pending_messages());

    network.tick(Duration::from_millis(200));
    println!("\n  ⏱️  After 300ms total:");
    println!("    Messages delivered: {}", network.stats().messages_delivered);
    println!("    Average latency: {:.1}ms", network.stats().avg_latency_ms);

    // Network partition demo
    println!("\n  🔌 Network Partition Simulation:");
    println!("    (Simulates when network splits into isolated groups)");

    let group_a: Vec<_> = validator_ids[..2].to_vec();
    let group_b: Vec<_> = validator_ids[2..].to_vec();

    println!();
}
