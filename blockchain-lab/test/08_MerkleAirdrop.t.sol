// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/08_MerkleAirdrop.sol";

/**
 * TEST: MERKLE PROOFS & AIRDROPS
 */
contract MerkleAirdropTest is Test {
    MerkleAirdrop public airdrop;
    MerkleTreeHelper public helper;

    // Test addresses
    address public alice = address(0x1);
    address public bob = address(0x2);
    address public charlie = address(0x3);
    address public dave = address(0x4);

    // Airdrop amounts
    uint256 public aliceAmount = 100 ether;
    uint256 public bobAmount = 200 ether;
    uint256 public charlieAmount = 150 ether;
    uint256 public daveAmount = 50 ether;

    // Merkle tree components
    bytes32 public leaf1;
    bytes32 public leaf2;
    bytes32 public leaf3;
    bytes32 public leaf4;
    bytes32 public hash12;
    bytes32 public hash34;
    bytes32 public merkleRoot;

    function setUp() public {
        helper = new MerkleTreeHelper();

        // Build the Merkle tree manually
        leaf1 = keccak256(abi.encodePacked(alice, aliceAmount));
        leaf2 = keccak256(abi.encodePacked(bob, bobAmount));
        leaf3 = keccak256(abi.encodePacked(charlie, charlieAmount));
        leaf4 = keccak256(abi.encodePacked(dave, daveAmount));

        // Hash pairs (sorted)
        hash12 = _sortAndHash(leaf1, leaf2);
        hash34 = _sortAndHash(leaf3, leaf4);
        merkleRoot = _sortAndHash(hash12, hash34);

        // Deploy airdrop with computed root
        airdrop = new MerkleAirdrop(merkleRoot, address(0)); // token not used in tests
    }

    function _sortAndHash(bytes32 a, bytes32 b) internal pure returns (bytes32) {
        return a <= b
            ? keccak256(abi.encodePacked(a, b))
            : keccak256(abi.encodePacked(b, a));
    }

    /**
     * TEST: Verify proof for Alice
     */
    function testVerifyProofAlice() public view {
        // Alice's proof: [leaf2, hash34]
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf2;
        proof[1] = hash34;

        bool isValid = airdrop.verifyProof(proof, merkleRoot, leaf1);
        assertTrue(isValid, "Alice's proof should be valid");
    }

    /**
     * TEST: Verify proof for Bob
     */
    function testVerifyProofBob() public view {
        // Bob's proof: [leaf1, hash34]
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf1;
        proof[1] = hash34;

        bool isValid = airdrop.verifyProof(proof, merkleRoot, leaf2);
        assertTrue(isValid, "Bob's proof should be valid");
    }

    /**
     * TEST: Verify proof for Charlie
     */
    function testVerifyProofCharlie() public view {
        // Charlie's proof: [leaf4, hash12]
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf4;
        proof[1] = hash12;

        bool isValid = airdrop.verifyProof(proof, merkleRoot, leaf3);
        assertTrue(isValid, "Charlie's proof should be valid");
    }

    /**
     * TEST: Verify proof for Dave
     */
    function testVerifyProofDave() public view {
        // Dave's proof: [leaf3, hash12]
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf3;
        proof[1] = hash12;

        bool isValid = airdrop.verifyProof(proof, merkleRoot, leaf4);
        assertTrue(isValid, "Dave's proof should be valid");
    }

    /**
     * TEST: Invalid proof fails
     */
    function testInvalidProofFails() public view {
        // Wrong proof for Alice
        bytes32[] memory wrongProof = new bytes32[](2);
        wrongProof[0] = leaf3; // Wrong sibling
        wrongProof[1] = hash34;

        bool isValid = airdrop.verifyProof(wrongProof, merkleRoot, leaf1);
        assertFalse(isValid, "Wrong proof should be invalid");
    }

    /**
     * TEST: Empty proof fails
     */
    function testEmptyProofFails() public view {
        bytes32[] memory emptyProof = new bytes32[](0);

        bool isValid = airdrop.verifyProof(emptyProof, merkleRoot, leaf1);
        assertFalse(isValid, "Empty proof should be invalid");
    }

    /**
     * TEST: Wrong amount fails
     */
    function testWrongAmountFails() public view {
        // Create leaf with wrong amount
        bytes32 wrongLeaf = keccak256(abi.encodePacked(alice, uint256(999 ether)));

        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf2;
        proof[1] = hash34;

        bool isValid = airdrop.verifyProof(proof, merkleRoot, wrongLeaf);
        assertFalse(isValid, "Wrong amount should fail");
    }

    /**
     * TEST: Claim airdrop
     */
    function testClaim() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf2;
        proof[1] = hash34;

        // Claim as Alice
        vm.prank(alice);
        airdrop.claim(alice, aliceAmount, proof);

        assertTrue(airdrop.hasClaimed(alice), "Alice should have claimed");
    }

    /**
     * TEST: Cannot claim twice
     */
    function testCannotClaimTwice() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf2;
        proof[1] = hash34;

        // First claim
        vm.prank(alice);
        airdrop.claim(alice, aliceAmount, proof);

        // Second claim should fail
        vm.prank(alice);
        vm.expectRevert("Already claimed");
        airdrop.claim(alice, aliceAmount, proof);
    }

    /**
     * TEST: Cannot claim with invalid proof
     */
    function testCannotClaimWithInvalidProof() public {
        bytes32[] memory wrongProof = new bytes32[](2);
        wrongProof[0] = leaf3; // Wrong
        wrongProof[1] = hash34;

        vm.prank(alice);
        vm.expectRevert("Invalid proof");
        airdrop.claim(alice, aliceAmount, wrongProof);
    }

    /**
     * TEST: MerkleTreeHelper computeLeaf
     */
    function testComputeLeaf() public view {
        bytes32 computed = helper.computeLeaf(alice, aliceAmount);
        assertEq(computed, leaf1, "Leaf should match");
    }

    /**
     * TEST: MerkleTreeHelper computeParent
     */
    function testComputeParent() public view {
        bytes32 computed = helper.computeParent(leaf1, leaf2);
        assertEq(computed, hash12, "Parent hash should match");
    }

    /**
     * TEST: Anyone can submit claim on behalf of user
     */
    function testAnyoneCanSubmitClaim() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leaf2;
        proof[1] = hash34;

        // Bob submits Alice's claim
        vm.prank(bob);
        airdrop.claim(alice, aliceAmount, proof);

        assertTrue(airdrop.hasClaimed(alice), "Alice should have claimed");
    }

    /**
     * TEST: Multiple users can claim
     */
    function testMultipleUsersClaim() public {
        // Alice claims
        bytes32[] memory aliceProof = new bytes32[](2);
        aliceProof[0] = leaf2;
        aliceProof[1] = hash34;
        airdrop.claim(alice, aliceAmount, aliceProof);

        // Bob claims
        bytes32[] memory bobProof = new bytes32[](2);
        bobProof[0] = leaf1;
        bobProof[1] = hash34;
        airdrop.claim(bob, bobAmount, bobProof);

        // Dave claims
        bytes32[] memory daveProof = new bytes32[](2);
        daveProof[0] = leaf3;
        daveProof[1] = hash12;
        airdrop.claim(dave, daveAmount, daveProof);

        assertTrue(airdrop.hasClaimed(alice), "Alice should have claimed");
        assertTrue(airdrop.hasClaimed(bob), "Bob should have claimed");
        assertTrue(airdrop.hasClaimed(dave), "Dave should have claimed");
        assertFalse(airdrop.hasClaimed(charlie), "Charlie should not have claimed");
    }
}
