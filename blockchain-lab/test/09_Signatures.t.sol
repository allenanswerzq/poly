// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/09_Signatures.sol";

/**
 * TEST: SIGNATURES & EIP-712
 */
contract SignatureVerifierTest is Test {
    SignatureVerifier public verifier;

    // Test accounts with known private keys
    uint256 constant ALICE_PK = 0x1;
    uint256 constant BOB_PK = 0x2;
    address public alice;
    address public bob;

    function setUp() public {
        verifier = new SignatureVerifier();

        // Derive addresses from private keys
        alice = vm.addr(ALICE_PK);
        bob = vm.addr(BOB_PK);

        // Fund accounts
        vm.deal(alice, 100 ether);
        vm.deal(bob, 100 ether);

        // Alice deposits
        vm.prank(alice);
        verifier.deposit{value: 10 ether}();
    }

    /**
     * TEST: Transfer with valid signature
     */
    function testTransferWithSignature() public {
        uint256 amount = 1 ether;
        uint256 deadline = block.timestamp + 1 hours;

        // Get digest to sign
        bytes32 digest = verifier.getTransferDigest(alice, bob, amount, deadline);

        // Alice signs
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // Anyone can submit (Bob submits)
        vm.prank(bob);
        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);

        assertEq(verifier.balances(alice), 9 ether);
        assertEq(verifier.balances(bob), 1 ether);
    }

    /**
     * TEST: Expired signature fails
     */
    function testExpiredSignatureFails() public {
        uint256 amount = 1 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = verifier.getTransferDigest(alice, bob, amount, deadline);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // Time travel past deadline
        vm.warp(block.timestamp + 2 hours);

        vm.expectRevert("Signature expired");
        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);
    }

    /**
     * TEST: Invalid signature fails
     */
    function testInvalidSignatureFails() public {
        uint256 amount = 1 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = verifier.getTransferDigest(alice, bob, amount, deadline);

        // Bob signs (wrong signer)
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(BOB_PK, digest);

        vm.expectRevert("Invalid signature");
        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);
    }

    /**
     * TEST: Nonce prevents replay
     */
    function testNoncePreventsReplay() public {
        uint256 amount = 1 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = verifier.getTransferDigest(alice, bob, amount, deadline);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // First transfer succeeds
        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);

        // Replay attack fails (nonce incremented)
        vm.expectRevert("Invalid signature");
        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);
    }

    /**
     * TEST: Nonce increments correctly
     */
    function testNonceIncrements() public {
        assertEq(verifier.nonces(alice), 0);

        uint256 amount = 1 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = verifier.getTransferDigest(alice, bob, amount, deadline);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);

        assertEq(verifier.nonces(alice), 1);
    }

    /**
     * TEST: Insufficient balance fails
     */
    function testInsufficientBalanceFails() public {
        uint256 amount = 100 ether; // More than deposited
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = verifier.getTransferDigest(alice, bob, amount, deadline);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        vm.expectRevert("Insufficient balance");
        verifier.transferWithSignature(alice, bob, amount, deadline, v, r, s);
    }

    /**
     * TEST: Simple signature verification
     */
    function testSimpleSignatureVerification() public view {
        bytes32 messageHash = keccak256("Hello, World!");

        // Sign with Ethereum prefix
        bytes32 ethSignedHash = keccak256(
            abi.encodePacked("\x19Ethereum Signed Message:\n32", messageHash)
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, ethSignedHash);

        address recovered = verifier.verifySimpleSignature(messageHash, v, r, s);
        assertEq(recovered, alice);
    }

    /**
     * TEST: Multiple sequential transfers
     */
    function testMultipleTransfers() public {
        uint256 deadline = block.timestamp + 1 hours;

        // First transfer
        bytes32 digest1 = verifier.getTransferDigest(alice, bob, 1 ether, deadline);
        (uint8 v1, bytes32 r1, bytes32 s1) = vm.sign(ALICE_PK, digest1);
        verifier.transferWithSignature(alice, bob, 1 ether, deadline, v1, r1, s1);

        // Second transfer (nonce = 1)
        bytes32 digest2 = verifier.getTransferDigest(alice, bob, 2 ether, deadline);
        (uint8 v2, bytes32 r2, bytes32 s2) = vm.sign(ALICE_PK, digest2);
        verifier.transferWithSignature(alice, bob, 2 ether, deadline, v2, r2, s2);

        assertEq(verifier.balances(alice), 7 ether);
        assertEq(verifier.balances(bob), 3 ether);
        assertEq(verifier.nonces(alice), 2);
    }
}

/**
 * TEST: ERC20 WITH PERMIT
 */
contract ERC20WithPermitTest is Test {
    ERC20WithPermit public token;

    uint256 constant ALICE_PK = 0x1;
    uint256 constant BOB_PK = 0x2;
    address public alice;
    address public bob;

    function setUp() public {
        token = new ERC20WithPermit();

        alice = vm.addr(ALICE_PK);
        bob = vm.addr(BOB_PK);

        // Mint tokens to Alice
        token.mint(alice, 1000 ether);
    }

    /**
     * TEST: Permit sets allowance
     */
    function testPermit() public {
        uint256 value = 100 ether;
        uint256 deadline = block.timestamp + 1 hours;
        uint256 nonce = token.nonces(alice);

        // Build the digest
        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                token.DOMAIN_SEPARATOR(),
                keccak256(
                    abi.encode(
                        token.PERMIT_TYPEHASH(),
                        alice,
                        bob,
                        value,
                        nonce,
                        deadline
                    )
                )
            )
        );

        // Alice signs
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // Bob calls permit
        vm.prank(bob);
        token.permit(alice, bob, value, deadline, v, r, s);

        assertEq(token.allowance(alice, bob), value);
    }

    /**
     * TEST: Permit and transferFrom in one flow
     */
    function testPermitAndTransfer() public {
        uint256 value = 100 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                token.DOMAIN_SEPARATOR(),
                keccak256(
                    abi.encode(
                        token.PERMIT_TYPEHASH(),
                        alice,
                        bob,
                        value,
                        token.nonces(alice),
                        deadline
                    )
                )
            )
        );

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // Bob: permit + transfer in same flow
        vm.startPrank(bob);
        token.permit(alice, bob, value, deadline, v, r, s);
        token.transferFrom(alice, bob, value);
        vm.stopPrank();

        assertEq(token.balanceOf(bob), value);
        assertEq(token.balanceOf(alice), 900 ether);
    }

    /**
     * TEST: Expired permit fails
     */
    function testExpiredPermitFails() public {
        uint256 value = 100 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                token.DOMAIN_SEPARATOR(),
                keccak256(
                    abi.encode(
                        token.PERMIT_TYPEHASH(),
                        alice,
                        bob,
                        value,
                        token.nonces(alice),
                        deadline
                    )
                )
            )
        );

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // Time travel past deadline
        vm.warp(block.timestamp + 2 hours);

        vm.expectRevert("Permit expired");
        token.permit(alice, bob, value, deadline, v, r, s);
    }

    /**
     * TEST: Invalid signer fails
     */
    function testInvalidSignerFails() public {
        uint256 value = 100 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                token.DOMAIN_SEPARATOR(),
                keccak256(
                    abi.encode(
                        token.PERMIT_TYPEHASH(),
                        alice,
                        bob,
                        value,
                        token.nonces(alice),
                        deadline
                    )
                )
            )
        );

        // Bob signs instead of Alice
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(BOB_PK, digest);

        vm.expectRevert("Invalid signature");
        token.permit(alice, bob, value, deadline, v, r, s);
    }

    /**
     * TEST: Nonce prevents permit replay
     */
    function testNoncePreventsPermitReplay() public {
        uint256 value = 100 ether;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                token.DOMAIN_SEPARATOR(),
                keccak256(
                    abi.encode(
                        token.PERMIT_TYPEHASH(),
                        alice,
                        bob,
                        value,
                        token.nonces(alice),
                        deadline
                    )
                )
            )
        );

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        // First permit succeeds
        token.permit(alice, bob, value, deadline, v, r, s);

        // Replay fails
        vm.expectRevert("Invalid signature");
        token.permit(alice, bob, value, deadline, v, r, s);
    }

    /**
     * TEST: Standard transfer works
     */
    function testStandardTransfer() public {
        vm.prank(alice);
        token.transfer(bob, 100 ether);

        assertEq(token.balanceOf(alice), 900 ether);
        assertEq(token.balanceOf(bob), 100 ether);
    }

    /**
     * TEST: Standard approve and transferFrom
     */
    function testStandardApproveTransferFrom() public {
        vm.prank(alice);
        token.approve(bob, 100 ether);

        vm.prank(bob);
        token.transferFrom(alice, bob, 100 ether);

        assertEq(token.balanceOf(bob), 100 ether);
    }
}
