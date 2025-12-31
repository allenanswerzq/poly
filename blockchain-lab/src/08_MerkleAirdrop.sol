// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 8: MERKLE PROOFS & AIRDROPS
 *
 * Learn how protocols distribute tokens to thousands of users
 * with only storing a single 32-byte hash on-chain.
 */

contract MerkleAirdrop {
    bytes32 public immutable merkleRoot;
    address public immutable token;

    // Track who has claimed
    mapping(address => bool) public hasClaimed;

    event Claimed(address indexed account, uint256 amount);

    constructor(bytes32 _merkleRoot, address _token) {
        merkleRoot = _merkleRoot;
        token = _token;
    }

    /**
     * CLAIM AIRDROP
     *
     * User provides:
     * - Their address and amount
     * - Merkle proof (array of hashes)
     *
     * We verify they're in the tree without storing all addresses!
     */
    function claim(
        address account,
        uint256 amount,
        bytes32[] calldata merkleProof
    ) external {
        require(!hasClaimed[account], "Already claimed");

        // Create leaf from account + amount
        bytes32 leaf = keccak256(abi.encodePacked(account, amount));

        // Verify the proof
        require(verifyProof(merkleProof, merkleRoot, leaf), "Invalid proof");

        // Mark as claimed
        hasClaimed[account] = true;

        // Transfer tokens
        // IERC20(token).transfer(account, amount);

        emit Claimed(account, amount);
    }

    /**
     * VERIFY MERKLE PROOF
     *
     * How it works:
     * 1. Start with the leaf (your claim)
     * 2. Hash it with sibling from proof
     * 3. Hash result with next sibling
     * 4. Continue until you reach the root
     * 5. If final hash matches root, proof is valid!
     */
    function verifyProof(
        bytes32[] calldata proof,
        bytes32 root,
        bytes32 leaf
    ) public pure returns (bool) {
        bytes32 computedHash = leaf;

        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 proofElement = proof[i];

            // Sort the pair before hashing (ensures consistency)
            if (computedHash <= proofElement) {
                computedHash = keccak256(abi.encodePacked(computedHash, proofElement));
            } else {
                computedHash = keccak256(abi.encodePacked(proofElement, computedHash));
            }
        }

        return computedHash == root;
    }
}

/**
 * MERKLE TREE BUILDER (for off-chain use)
 *
 * In practice, you'd build this in JavaScript/TypeScript:
 *
 * const leaves = [
 *   keccak256(abi.encodePacked(address1, amount1)),
 *   keccak256(abi.encodePacked(address2, amount2)),
 *   ...
 * ];
 *
 * const tree = new MerkleTree(leaves, keccak256, { sort: true });
 * const root = tree.getRoot();
 * const proof = tree.getProof(leaves[0]);
 */

/**
 * VISUAL EXAMPLE:
 *
 * Imagine 4 users in airdrop:
 *
 *                    ROOT (stored on-chain)
 *                   /    \
 *               Hash12    Hash34
 *              /    \    /    \
 *          Leaf1  Leaf2  Leaf3  Leaf4
 *          (A,100)(B,200)(C,150)(D,50)
 *
 * To prove A gets 100 tokens:
 * - Leaf1 = hash(A, 100)
 * - Proof = [Leaf2, Hash34]
 *
 * Verification:
 * 1. hash(Leaf1, Leaf2) = Hash12
 * 2. hash(Hash12, Hash34) = ROOT ✓
 *
 * We verified A's claim without storing A, B, C, or D on-chain!
 */

contract MerkleTreeHelper {
    /**
     * Helper to compute a leaf hash
     */
    function computeLeaf(address account, uint256 amount) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(account, amount));
    }

    /**
     * Helper to compute parent hash from two children
     */
    function computeParent(bytes32 left, bytes32 right) external pure returns (bytes32) {
        if (left <= right) {
            return keccak256(abi.encodePacked(left, right));
        } else {
            return keccak256(abi.encodePacked(right, left));
        }
    }

    /**
     * Example: Build tree for 4 users
     */
    function buildExampleTree() external pure returns (
        bytes32 leaf1,
        bytes32 leaf2,
        bytes32 leaf3,
        bytes32 leaf4,
        bytes32 hash12,
        bytes32 hash34,
        bytes32 root
    ) {
        // Create leaves
        leaf1 = keccak256(abi.encodePacked(address(0x1), uint256(100)));
        leaf2 = keccak256(abi.encodePacked(address(0x2), uint256(200)));
        leaf3 = keccak256(abi.encodePacked(address(0x3), uint256(150)));
        leaf4 = keccak256(abi.encodePacked(address(0x4), uint256(50)));

        // Build tree bottom-up
        hash12 = _sortAndHash(leaf1, leaf2);
        hash34 = _sortAndHash(leaf3, leaf4);
        root = _sortAndHash(hash12, hash34);
    }

    function _sortAndHash(bytes32 a, bytes32 b) internal pure returns (bytes32) {
        return a <= b
            ? keccak256(abi.encodePacked(a, b))
            : keccak256(abi.encodePacked(b, a));
    }
}

/**
 * WHY MERKLE TREES?
 *
 * PROBLEM: Airdrop to 100,000 users
 *
 * NAIVE SOLUTION:
 * - Store all 100,000 addresses on-chain
 * - Cost: ~100,000 * 20,000 gas = 2 BILLION gas
 * - At $50/M gas = $100,000 just for storage!
 *
 * MERKLE SOLUTION:
 * - Store single 32-byte root: ~20,000 gas
 * - Users prove their inclusion with ~10 hashes
 * - Total cost: ~$1
 *
 * This is how Uniswap, 1inch, ENS did their airdrops!
 */
