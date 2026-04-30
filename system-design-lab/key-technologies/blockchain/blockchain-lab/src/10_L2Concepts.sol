// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 10: L2 CONCEPTS - STATE CHANNELS & ROLLUPS
 *
 * Understand L2 scaling through code.
 */

/**
 * SIMPLE STATE CHANNEL
 *
 * Two parties can transact off-chain, only settling on-chain
 * when they're done or there's a dispute.
 */
contract PaymentChannel {
    address public immutable sender;
    address public immutable recipient;
    uint256 public immutable expiration;

    bool public closed;

    constructor(address _recipient, uint256 duration) payable {
        sender = msg.sender;
        recipient = _recipient;
        expiration = block.timestamp + duration;
    }

    /**
     * OFF-CHAIN PAYMENTS
     *
     * Sender signs messages off-chain:
     * - "Recipient can claim 1 ETH" (signed)
     * - "Recipient can claim 2 ETH" (signed)
     * - "Recipient can claim 3 ETH" (signed)
     *
     * Only final claim goes on-chain!
     */

    /**
     * CLOSE CHANNEL
     *
     * Recipient submits the highest-value signed message.
     */
    function close(uint256 amount, bytes memory signature) external {
        require(msg.sender == recipient, "Only recipient");
        require(!closed, "Already closed");

        // Verify signature
        bytes32 message = prefixed(keccak256(abi.encodePacked(address(this), amount)));
        require(recoverSigner(message, signature) == sender, "Invalid signature");

        closed = true;

        // Pay recipient
        payable(recipient).transfer(amount);

        // Refund remainder to sender
        if (address(this).balance > 0) {
            payable(sender).transfer(address(this).balance);
        }
    }

    /**
     * TIMEOUT
     *
     * If recipient doesn't close, sender can reclaim after expiration.
     */
    function claimTimeout() external {
        require(block.timestamp >= expiration, "Not expired");
        require(!closed, "Already closed");

        closed = true;
        payable(sender).transfer(address(this).balance);
    }

    // Signature helpers
    function prefixed(bytes32 hash) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked("\x19Ethereum Signed Message:\n32", hash));
    }

    function recoverSigner(bytes32 message, bytes memory sig) internal pure returns (address) {
        require(sig.length == 65, "Invalid signature length");

        bytes32 r;
        bytes32 s;
        uint8 v;

        assembly {
            r := mload(add(sig, 32))
            s := mload(add(sig, 64))
            v := byte(0, mload(add(sig, 96)))
        }

        return ecrecover(message, v, r, s);
    }

    /**
     * HELPER: Get message hash for signing
     */
    function getMessageHash(uint256 amount) external view returns (bytes32) {
        return keccak256(abi.encodePacked(address(this), amount));
    }
}

/**
 * SIMPLIFIED ROLLUP CONCEPT
 *
 * This demonstrates the core idea of a rollup:
 * - Batch transactions off-chain
 * - Submit compressed data on-chain
 * - Verify with fraud proof OR validity proof
 */
contract SimpleRollup {
    // Current state root (Merkle root of all account states)
    bytes32 public stateRoot;

    // Batch counter
    uint256 public batchNumber;

    // Sequencer (in practice, would be decentralized)
    address public sequencer;

    // For fraud proofs: store batch data for challenge period
    struct Batch {
        bytes32 prevStateRoot;
        bytes32 newStateRoot;
        bytes32 txDataHash;
        uint256 timestamp;
        bool finalized;
    }

    mapping(uint256 => Batch) public batches;

    // Challenge period (simplified: 1 hour for demo, real: 7 days)
    uint256 public constant CHALLENGE_PERIOD = 1 hours;

    event BatchSubmitted(uint256 indexed batchNumber, bytes32 newStateRoot);
    event BatchChallenged(uint256 indexed batchNumber, address challenger);
    event BatchFinalized(uint256 indexed batchNumber);

    constructor(bytes32 _initialStateRoot) {
        stateRoot = _initialStateRoot;
        sequencer = msg.sender;
    }

    /**
     * SUBMIT BATCH
     *
     * Sequencer submits:
     * - New state root after executing all txs
     * - Compressed transaction data
     */
    function submitBatch(
        bytes32 newStateRoot,
        bytes calldata compressedTxData
    ) external {
        require(msg.sender == sequencer, "Only sequencer");

        batches[batchNumber] = Batch({
            prevStateRoot: stateRoot,
            newStateRoot: newStateRoot,
            txDataHash: keccak256(compressedTxData),
            timestamp: block.timestamp,
            finalized: false
        });

        // Optimistically accept the new state
        stateRoot = newStateRoot;

        emit BatchSubmitted(batchNumber, newStateRoot);
        batchNumber++;
    }

    /**
     * CHALLENGE BATCH (Optimistic Rollup)
     *
     * If anyone finds an invalid state transition,
     * they can prove it and revert the batch.
     */
    function challengeBatch(
        uint256 _batchNumber,
        bytes calldata fraudProof
    ) external {
        Batch storage batch = batches[_batchNumber];
        require(!batch.finalized, "Already finalized");
        require(block.timestamp < batch.timestamp + CHALLENGE_PERIOD, "Challenge period over");

        // In a real rollup, we'd verify the fraud proof here
        // This would involve re-executing the disputed transaction
        // and checking if the state transition was invalid

        // For demo, assume fraudProof is valid if non-empty
        require(fraudProof.length > 0, "Invalid proof");

        // Revert to previous state
        stateRoot = batch.prevStateRoot;

        // Slash sequencer (in practice, they'd lose their stake)

        emit BatchChallenged(_batchNumber, msg.sender);
    }

    /**
     * FINALIZE BATCH
     *
     * After challenge period, batch is finalized.
     * This is when withdrawals become available.
     */
    function finalizeBatch(uint256 _batchNumber) external {
        Batch storage batch = batches[_batchNumber];
        require(!batch.finalized, "Already finalized");
        require(
            block.timestamp >= batch.timestamp + CHALLENGE_PERIOD,
            "Challenge period not over"
        );

        batch.finalized = true;

        emit BatchFinalized(_batchNumber);
    }

    /**
     * DEPOSIT (L1 -> L2)
     */
    function deposit() external payable {
        // In a real rollup:
        // 1. Lock funds in this contract
        // 2. Emit event
        // 3. Sequencer includes deposit in next batch
        // 4. User gets funds on L2
    }

    /**
     * WITHDRAW (L2 -> L1)
     */
    function withdraw(
        uint256 amount,
        bytes32[] calldata merkleProof
    ) external {
        // In a real rollup:
        // 1. User submits withdrawal on L2
        // 2. Wait for batch to be finalized
        // 3. Provide Merkle proof that withdrawal is in state
        // 4. Claim funds on L1
    }
}

/**
 * L2 COMPARISON
 *
 * OPTIMISTIC ROLLUP (Arbitrum, Optimism):
 * ┌─────────────────────────────────────────────────┐
 * │ 1. Sequencer executes transactions             │
 * │ 2. Posts state root + compressed data to L1    │
 * │ 3. 7 day challenge period                      │
 * │ 4. Anyone can submit fraud proof               │
 * │ 5. If no challenge → finalized                 │
 * └─────────────────────────────────────────────────┘
 *
 * ZK ROLLUP (zkSync, StarkNet):
 * ┌─────────────────────────────────────────────────┐
 * │ 1. Sequencer executes transactions             │
 * │ 2. Generates ZK validity proof                 │
 * │ 3. Posts proof + compressed data to L1         │
 * │ 4. L1 verifies proof (instant!)                │
 * │ 5. No challenge period needed                  │
 * └─────────────────────────────────────────────────┘
 *
 * STATE CHANNEL:
 * ┌─────────────────────────────────────────────────┐
 * │ 1. Lock funds in L1 contract                   │
 * │ 2. Transact off-chain with signatures          │
 * │ 3. Only final state goes on-chain              │
 * │ 4. Dispute mechanism for cheating              │
 * └─────────────────────────────────────────────────┘
 */
