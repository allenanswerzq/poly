// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/05_AccessControl.sol";

/**
 * EXERCISE 5: ACCESS CONTROL ATTACKS
 *
 * Run with: forge test --match-contract AccessControlTest -vvvv
 */
contract AccessControlTest is Test {
    MissingAccessControl public vulnerable1;
    TxOriginPhishing public vulnerable2;
    TxOriginAttacker public attacker2;

    address public owner = makeAddr("owner");
    address public attacker = makeAddr("attacker");
    address public victim = makeAddr("victim");

    function setUp() public {
        vm.startPrank(owner);
        vulnerable1 = new MissingAccessControl();
        vulnerable2 = new TxOriginPhishing();
        vm.stopPrank();

        // Fund vulnerable2
        vm.deal(address(vulnerable2), 10 ether);
    }

    // ============================================
    // TEST: Missing Access Control
    // ============================================

    function testMissingAccessControl() public {
        console.log("=== Missing Access Control Attack ===\n");

        console.log("Owner before:", vulnerable1.owner());
        assertEq(vulnerable1.owner(), owner);

        // Attacker takes over!
        vm.prank(attacker);
        vulnerable1.setOwner(attacker);

        console.log("Owner after:", vulnerable1.owner());
        assertEq(vulnerable1.owner(), attacker);

        console.log("\n!!! Attacker is now owner !!!");
    }

    // ============================================
    // TEST: tx.origin Phishing
    // ============================================

    function testTxOriginPhishing() public {
        console.log("\n=== tx.origin Phishing Attack ===\n");

        // Attacker deploys malicious contract
        vm.prank(attacker);
        attacker2 = new TxOriginAttacker(address(vulnerable2));

        console.log("Vulnerable contract balance:", address(vulnerable2).balance / 1e18, "ETH");
        console.log("Attacker balance before:", attacker.balance / 1e18, "ETH");

        // Owner is tricked into calling attacker's contract
        // Maybe it's disguised as "claim airdrop" or "verify account"
        console.log("\n>>> Owner calls malicious 'claimAirdrop()'...");

        vm.prank(owner, owner);  // Set both msg.sender AND tx.origin to owner
        attacker2.claimAirdrop();

        console.log("\nVulnerable contract balance:", address(vulnerable2).balance / 1e18, "ETH");
        console.log("Attacker balance after:", attacker.balance / 1e18, "ETH");

        assertEq(address(vulnerable2).balance, 0);
        assertEq(attacker.balance, 10 ether);

        console.log("\n!!! Funds stolen via tx.origin phishing !!!");
    }

    // ============================================
    // TEST: Unprotected Initialize
    // ============================================

    function testUnprotectedInitialize() public {
        console.log("\n=== Unprotected Initialize Attack ===\n");

        UnprotectedInitialize impl = new UnprotectedInitialize();

        console.log("Initialized:", impl.initialized());
        console.log("Owner:", impl.owner());

        // In a real scenario, this would be a proxy deployment
        // Attacker front-runs the legitimate initialize() call

        console.log("\n>>> Attacker front-runs initialize()...");
        vm.prank(attacker);
        impl.initialize(attacker);

        console.log("Initialized:", impl.initialized());
        console.log("Owner:", impl.owner());

        assertEq(impl.owner(), attacker);

        // Now legitimate owner tries to initialize - fails!
        vm.prank(owner);
        vm.expectRevert("Already initialized");
        impl.initialize(owner);

        console.log("\n!!! Attacker is owner, real owner locked out !!!");
    }

    // ============================================
    // DEMONSTRATION: Proper Access Control
    // ============================================

    function testSecureAccessControl() public {
        console.log("\n=== Secure Access Control ===\n");

        vm.startPrank(owner);
        SecureAccessControl secure = new SecureAccessControl();
        vm.stopPrank();

        console.log("Owner:", secure.owner());

        // Attacker tries to transfer ownership
        console.log("\n>>> Attacker tries to take over...");
        vm.prank(attacker);
        vm.expectRevert("Not owner");
        secure.transferOwnership(attacker);

        console.log("Attack failed! Access control working.");

        // Owner can properly transfer
        vm.prank(owner);
        secure.transferOwnership(victim);

        assertEq(secure.owner(), victim);
        console.log("Owner successfully transferred to:", secure.owner());
    }
}

// ============================================
// BONUS: Role-Based Access Control Demo
// ============================================
contract RBACDemo is Test {
    function testRBACConcepts() public pure {
        console.log("\n=== Role-Based Access Control (RBAC) ===\n");

        console.log("Instead of just 'owner', define ROLES:");
        console.log("");
        console.log("  ADMIN_ROLE");
        console.log("    - Can add/remove other roles");
        console.log("    - Can pause contract");
        console.log("");
        console.log("  MINTER_ROLE");
        console.log("    - Can mint new tokens");
        console.log("");
        console.log("  PAUSER_ROLE");
        console.log("    - Can pause/unpause");
        console.log("");
        console.log("  UPGRADER_ROLE");
        console.log("    - Can upgrade proxy implementation");
        console.log("");
        console.log("Use OpenZeppelin AccessControl:");
        console.log("  import '@openzeppelin/contracts/access/AccessControl.sol'");
        console.log("");
        console.log("Benefits:");
        console.log("  - Separation of concerns");
        console.log("  - Minimize attack surface");
        console.log("  - If one key compromised, limited damage");
    }
}
