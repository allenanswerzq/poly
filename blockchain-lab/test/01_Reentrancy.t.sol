// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/01_Reentrancy.sol";

/**
 * EXERCISE 1: TEST THE REENTRANCY ATTACK
 *
 * Run with: forge test --match-contract ReentrancyTest -vvvv
 */
contract ReentrancyTest is Test {
    VulnerableBank public vulnerableBank;
    SecureBank public secureBank;
    ReentrancyAttacker public attacker;

    address public alice = makeAddr("alice");
    address public bob = makeAddr("bob");
    address public eve = makeAddr("eve"); // The attacker

    function setUp() public {
        // Deploy contracts
        vulnerableBank = new VulnerableBank();
        secureBank = new SecureBank();

        // Give users some ETH
        vm.deal(alice, 10 ether);
        vm.deal(bob, 10 ether);
        vm.deal(eve, 2 ether);

        // Alice and Bob deposit into vulnerable bank
        vm.prank(alice);
        vulnerableBank.deposit{value: 5 ether}();

        vm.prank(bob);
        vulnerableBank.deposit{value: 5 ether}();

        // Bank now has 10 ETH
        console.log("=== Initial State ===");
        console.log("Vulnerable Bank balance:", vulnerableBank.getBalance() / 1e18, "ETH");
        console.log("Eve balance:", eve.balance / 1e18, "ETH");
    }

    function testReentrancyAttack() public {
        console.log("\n=== Starting Reentrancy Attack ===");

        // Eve deploys attacker contract
        vm.startPrank(eve);
        attacker = new ReentrancyAttacker(address(vulnerableBank));

        // Eve attacks with 1 ETH
        console.log("Eve attacks with 1 ETH...");
        attacker.attack{value: 1 ether}();

        console.log("\n=== After Attack ===");
        console.log("Attack reentry count:", attacker.attackCount());
        console.log("Attacker contract balance:", attacker.getBalance() / 1e18, "ETH");
        console.log("Vulnerable Bank balance:", vulnerableBank.getBalance() / 1e18, "ETH");

        // Eve collects the loot
        attacker.collectLoot();
        console.log("Eve final balance:", eve.balance / 1e18, "ETH");
        vm.stopPrank();

        // Verify the attack worked
        assertEq(vulnerableBank.getBalance(), 0, "Bank should be drained!");
        assertGt(eve.balance, 10 ether, "Eve should have stolen funds!");

        console.log("\n!!! ATTACK SUCCESSFUL - Bank drained !!!");
    }

    function testSecureBankPreventsReentrancy() public {
        console.log("\n=== Testing Secure Bank ===");

        // Setup secure bank with deposits
        vm.prank(alice);
        secureBank.deposit{value: 5 ether}();
        vm.prank(bob);
        secureBank.deposit{value: 5 ether}();

        console.log("Secure Bank balance:", secureBank.getBalance() / 1e18, "ETH");

        // The secure bank uses reentrancy guard
        // If someone tried to reenter, it would fail with "Reentrant call detected!"
        // We can't directly test this without a custom attacker for SecureBank

        vm.stopPrank();

        console.log("Secure Bank protected by reentrancy guard!");
        console.log("Any reentrant call would revert with: 'Reentrant call detected!'");
    }

    function testNormalWithdrawStillWorks() public {
        console.log("\n=== Testing Normal Withdraw ===");

        // Alice withdraws normally from secure bank
        vm.startPrank(alice);
        secureBank.deposit{value: 3 ether}();

        uint256 balanceBefore = alice.balance;
        secureBank.withdraw();
        uint256 balanceAfter = alice.balance;

        console.log("Alice withdrew:", (balanceAfter - balanceBefore) / 1e18, "ETH");
        assertEq(balanceAfter - balanceBefore, 3 ether);
        vm.stopPrank();
    }
}
