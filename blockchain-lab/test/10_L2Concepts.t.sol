// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/10_L2Concepts.sol";

/**
 * TEST: STATE CHANNELS
 */
contract PaymentChannelTest is Test {
    PaymentChannel public channel;

    uint256 constant SENDER_PK = 0x1;
    uint256 constant RECIPIENT_PK = 0x2;
    address public sender;
    address public recipient;

    uint256 constant CHANNEL_DEPOSIT = 10 ether;
    uint256 constant DURATION = 1 days;

    function setUp() public {
        sender = vm.addr(SENDER_PK);
        recipient = vm.addr(RECIPIENT_PK);

        vm.deal(sender, 100 ether);

        vm.prank(sender);
        channel = new PaymentChannel{value: CHANNEL_DEPOSIT}(recipient, DURATION);
    }

    /**
     * HELPER: Sign a payment message
     */
    function _signPayment(uint256 amount) internal view returns (bytes memory) {
        bytes32 messageHash = channel.getMessageHash(amount);
        bytes32 ethSignedHash = keccak256(
            abi.encodePacked("\x19Ethereum Signed Message:\n32", messageHash)
        );

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(SENDER_PK, ethSignedHash);
        return abi.encodePacked(r, s, v);
    }

    /**
     * TEST: Channel deploys correctly
     */
    function testChannelDeployment() public view {
        assertEq(channel.sender(), sender);
        assertEq(channel.recipient(), recipient);
        assertEq(address(channel).balance, CHANNEL_DEPOSIT);
        assertFalse(channel.closed());
    }

    /**
     * TEST: Recipient can close with valid signature
     */
    function testRecipientCanClose() public {
        uint256 claimAmount = 3 ether;
        bytes memory signature = _signPayment(claimAmount);

        uint256 recipientBalanceBefore = recipient.balance;
        uint256 senderBalanceBefore = sender.balance;

        vm.prank(recipient);
        channel.close(claimAmount, signature);

        assertTrue(channel.closed());
        assertEq(recipient.balance, recipientBalanceBefore + claimAmount);
        assertEq(sender.balance, senderBalanceBefore + (CHANNEL_DEPOSIT - claimAmount));
    }

    /**
     * TEST: Can claim full amount
     */
    function testClaimFullAmount() public {
        bytes memory signature = _signPayment(CHANNEL_DEPOSIT);

        vm.prank(recipient);
        channel.close(CHANNEL_DEPOSIT, signature);

        assertEq(recipient.balance, CHANNEL_DEPOSIT);
    }

    /**
     * TEST: Only recipient can close
     */
    function testOnlyRecipientCanClose() public {
        bytes memory signature = _signPayment(1 ether);

        vm.prank(sender);
        vm.expectRevert("Only recipient");
        channel.close(1 ether, signature);
    }

    /**
     * TEST: Cannot close twice
     */
    function testCannotCloseTwice() public {
        bytes memory signature = _signPayment(1 ether);

        vm.prank(recipient);
        channel.close(1 ether, signature);

        vm.prank(recipient);
        vm.expectRevert("Already closed");
        channel.close(1 ether, signature);
    }

    /**
     * TEST: Invalid signature fails
     */
    function testInvalidSignatureFails() public {
        // Recipient signs (wrong signer)
        bytes32 messageHash = channel.getMessageHash(1 ether);
        bytes32 ethSignedHash = keccak256(
            abi.encodePacked("\x19Ethereum Signed Message:\n32", messageHash)
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(RECIPIENT_PK, ethSignedHash);
        bytes memory wrongSig = abi.encodePacked(r, s, v);

        vm.prank(recipient);
        vm.expectRevert("Invalid signature");
        channel.close(1 ether, wrongSig);
    }

    /**
     * TEST: Wrong amount fails
     */
    function testWrongAmountFails() public {
        bytes memory signature = _signPayment(1 ether);

        vm.prank(recipient);
        vm.expectRevert("Invalid signature");
        channel.close(2 ether, signature); // Trying to claim more
    }

    /**
     * TEST: Sender can reclaim after timeout
     */
    function testSenderCanReclaimAfterTimeout() public {
        uint256 senderBalanceBefore = sender.balance;

        // Time travel past expiration
        vm.warp(block.timestamp + DURATION + 1);

        vm.prank(sender);
        channel.claimTimeout();

        assertTrue(channel.closed());
        assertEq(sender.balance, senderBalanceBefore + CHANNEL_DEPOSIT);
    }

    /**
     * TEST: Cannot claim timeout before expiration
     */
    function testCannotClaimTimeoutEarly() public {
        vm.prank(sender);
        vm.expectRevert("Not expired");
        channel.claimTimeout();
    }

    /**
     * TEST: Cannot timeout after close
     */
    function testCannotTimeoutAfterClose() public {
        bytes memory signature = _signPayment(1 ether);

        vm.prank(recipient);
        channel.close(1 ether, signature);

        vm.warp(block.timestamp + DURATION + 1);

        vm.prank(sender);
        vm.expectRevert("Already closed");
        channel.claimTimeout();
    }

    /**
     * TEST: Message hash is deterministic
     */
    function testMessageHashDeterministic() public view {
        bytes32 hash1 = channel.getMessageHash(1 ether);
        bytes32 hash2 = channel.getMessageHash(1 ether);
        assertEq(hash1, hash2);

        bytes32 hash3 = channel.getMessageHash(2 ether);
        assertTrue(hash1 != hash3);
    }
}

/**
 * TEST: SIMPLE ROLLUP
 */
contract SimpleRollupTest is Test {
    SimpleRollup public rollup;

    address public sequencer;
    address public challenger;

    bytes32 public initialRoot = keccak256("initial_state");

    function setUp() public {
        sequencer = address(this);
        challenger = address(0x999);

        rollup = new SimpleRollup(initialRoot);

        vm.deal(sequencer, 100 ether);
        vm.deal(challenger, 100 ether);
    }

    /**
     * TEST: Rollup deploys correctly
     */
    function testRollupDeployment() public view {
        assertEq(rollup.stateRoot(), initialRoot);
        assertEq(rollup.batchNumber(), 0);
        assertEq(rollup.sequencer(), sequencer);
    }

    /**
     * TEST: Sequencer can submit batch
     */
    function testSubmitBatch() public {
        bytes32 newRoot = keccak256("new_state");
        bytes memory txData = abi.encode("tx1", "tx2", "tx3");

        rollup.submitBatch(newRoot, txData);

        assertEq(rollup.stateRoot(), newRoot);
        assertEq(rollup.batchNumber(), 1);
    }

    /**
     * TEST: Batch stores correct data
     */
    function testBatchStoredCorrectly() public {
        bytes32 newRoot = keccak256("new_state");
        bytes memory txData = abi.encode("transactions");

        rollup.submitBatch(newRoot, txData);

        (
            bytes32 prevStateRoot,
            bytes32 storedNewRoot,
            bytes32 txDataHash,
            uint256 timestamp,
            bool finalized
        ) = rollup.batches(0);

        assertEq(prevStateRoot, initialRoot);
        assertEq(storedNewRoot, newRoot);
        assertEq(txDataHash, keccak256(txData));
        assertEq(timestamp, block.timestamp);
        assertFalse(finalized);
    }

    /**
     * TEST: Only sequencer can submit
     */
    function testOnlySequencerCanSubmit() public {
        bytes32 newRoot = keccak256("new_state");

        vm.prank(challenger);
        vm.expectRevert("Only sequencer");
        rollup.submitBatch(newRoot, "");
    }

    /**
     * TEST: Multiple batches update correctly
     */
    function testMultipleBatches() public {
        bytes32 root1 = keccak256("state1");
        bytes32 root2 = keccak256("state2");
        bytes32 root3 = keccak256("state3");

        rollup.submitBatch(root1, "batch1");
        rollup.submitBatch(root2, "batch2");
        rollup.submitBatch(root3, "batch3");

        assertEq(rollup.stateRoot(), root3);
        assertEq(rollup.batchNumber(), 3);
    }

    /**
     * TEST: Challenge reverts state
     */
    function testChallengeRevertsState() public {
        bytes32 newRoot = keccak256("fraudulent_state");
        rollup.submitBatch(newRoot, "bad_txs");

        // Challenger submits fraud proof
        vm.prank(challenger);
        rollup.challengeBatch(0, "fraud_proof_data");

        // State reverted to initial
        assertEq(rollup.stateRoot(), initialRoot);
    }

    /**
     * TEST: Cannot challenge after period
     */
    function testCannotChallengeAfterPeriod() public {
        rollup.submitBatch(keccak256("new_state"), "txs");

        // Time travel past challenge period
        vm.warp(block.timestamp + rollup.CHALLENGE_PERIOD() + 1);

        vm.prank(challenger);
        vm.expectRevert("Challenge period over");
        rollup.challengeBatch(0, "proof");
    }

    /**
     * TEST: Cannot challenge finalized batch
     */
    function testCannotChallengeFinalizedBatch() public {
        rollup.submitBatch(keccak256("new_state"), "txs");

        // Wait and finalize
        vm.warp(block.timestamp + rollup.CHALLENGE_PERIOD() + 1);
        rollup.finalizeBatch(0);

        vm.prank(challenger);
        vm.expectRevert("Already finalized");
        rollup.challengeBatch(0, "proof");
    }

    /**
     * TEST: Empty fraud proof fails
     */
    function testEmptyFraudProofFails() public {
        rollup.submitBatch(keccak256("new_state"), "txs");

        vm.prank(challenger);
        vm.expectRevert("Invalid proof");
        rollup.challengeBatch(0, "");
    }

    /**
     * TEST: Finalize batch after challenge period
     */
    function testFinalizeBatch() public {
        rollup.submitBatch(keccak256("new_state"), "txs");

        // Wait for challenge period
        vm.warp(block.timestamp + rollup.CHALLENGE_PERIOD() + 1);

        rollup.finalizeBatch(0);

        (,,,,bool finalized) = rollup.batches(0);
        assertTrue(finalized);
    }

    /**
     * TEST: Cannot finalize early
     */
    function testCannotFinalizeEarly() public {
        rollup.submitBatch(keccak256("new_state"), "txs");

        vm.expectRevert("Challenge period not over");
        rollup.finalizeBatch(0);
    }

    /**
     * TEST: Cannot finalize twice
     */
    function testCannotFinalizeTwice() public {
        rollup.submitBatch(keccak256("new_state"), "txs");

        vm.warp(block.timestamp + rollup.CHALLENGE_PERIOD() + 1);
        rollup.finalizeBatch(0);

        vm.expectRevert("Already finalized");
        rollup.finalizeBatch(0);
    }

    /**
     * TEST: Challenge period constant
     */
    function testChallengePeriod() public view {
        assertEq(rollup.CHALLENGE_PERIOD(), 1 hours);
    }
}
