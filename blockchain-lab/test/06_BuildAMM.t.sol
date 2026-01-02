// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/06_BuildAMM.sol";
import "../src/tokens/ERC20.sol";

contract AMMTest is Test {
    SimpleAMM public amm;
    SimpleToken public tokenA;
    SimpleToken public tokenB;

    address public alice = makeAddr("alice");
    address public bob = makeAddr("bob");

    function setUp() public {
        // Deploy tokens (constructor: name, symbol, initialSupply)
        tokenA = new SimpleToken("Token A", "TKA", 0);
        tokenB = new SimpleToken("Token B", "TKB", 0);

        // Deploy AMM
        amm = new SimpleAMM(address(tokenA), address(tokenB));

        // Mint tokens to alice
        tokenA.mint(alice, 100_000 * 1e18);
        tokenB.mint(alice, 100_000 * 1e18);

        // Mint tokens to bob
        tokenA.mint(bob, 10_000 * 1e18);
        tokenB.mint(bob, 10_000 * 1e18);
    }

    function testAddLiquidityFirst() public {
        vm.startPrank(alice);

        // Approve AMM
        tokenA.approve(address(amm), 1000 * 1e18);
        tokenB.approve(address(amm), 2000 * 1e18);

        // Add liquidity
        uint256 liquidity = amm.addLiquidity(1000 * 1e18, 2000 * 1e18);

        // Check liquidity minted (sqrt(1000 * 2000) ≈ 1414)
        assertGt(liquidity, 0, "Should mint liquidity");
        assertEq(amm.liquidity(alice), liquidity, "Alice should have liquidity");
        assertEq(amm.reserveA(), 1000 * 1e18, "Reserve A should be 1000");
        assertEq(amm.reserveB(), 2000 * 1e18, "Reserve B should be 2000");

        vm.stopPrank();

        console.log("Liquidity minted:", liquidity / 1e18);
        console.log("Test PASSED: First liquidity provider");
    }

    function testAddLiquiditySecond() public {
        // First, alice adds liquidity
        vm.startPrank(alice);
        tokenA.approve(address(amm), 1000 * 1e18);
        tokenB.approve(address(amm), 2000 * 1e18);
        amm.addLiquidity(1000 * 1e18, 2000 * 1e18);
        vm.stopPrank();

        // Now bob adds liquidity (must match ratio)
        vm.startPrank(bob);
        tokenA.approve(address(amm), 500 * 1e18);
        tokenB.approve(address(amm), 1000 * 1e18);
        uint256 liquidity = amm.addLiquidity(500 * 1e18, 1000 * 1e18);
        vm.stopPrank();

        assertGt(liquidity, 0, "Bob should get liquidity");
        console.log("Bob's liquidity:", liquidity / 1e18);
        console.log("Test PASSED: Second liquidity provider");
    }

    function testSwapAForB() public {
        // Setup: Add liquidity
        vm.startPrank(alice);
        tokenA.approve(address(amm), 10000 * 1e18);
        tokenB.approve(address(amm), 10000 * 1e18);
        amm.addLiquidity(10000 * 1e18, 10000 * 1e18);
        vm.stopPrank();

        // Bob swaps tokenA for tokenB
        vm.startPrank(bob);
        tokenA.approve(address(amm), 1000 * 1e18);

        uint256 bobBBefore = tokenB.balanceOf(bob);
        uint256 amountOut = amm.swap(address(tokenA), 1000 * 1e18, 0);
        uint256 bobBAfter = tokenB.balanceOf(bob);

        assertGt(amountOut, 0, "Should get tokens out");
        assertEq(bobBAfter - bobBBefore, amountOut, "Balance should increase");

        // Due to slippage, should get less than 1000 tokenB
        assertLt(amountOut, 1000 * 1e18, "Slippage should reduce output");

        vm.stopPrank();

        console.log("Swapped 1000 tokenA for tokenB:", amountOut / 1e18);
        console.log("Test PASSED: Swap A for B");
    }

    function testSwapBForA() public {
        // Setup: Add liquidity
        vm.startPrank(alice);
        tokenA.approve(address(amm), 10000 * 1e18);
        tokenB.approve(address(amm), 10000 * 1e18);
        amm.addLiquidity(10000 * 1e18, 10000 * 1e18);
        vm.stopPrank();

        // Bob swaps tokenB for tokenA
        vm.startPrank(bob);
        tokenB.approve(address(amm), 1000 * 1e18);

        uint256 bobABefore = tokenA.balanceOf(bob);
        uint256 amountOut = amm.swap(address(tokenB), 1000 * 1e18, 0);
        uint256 bobAAfter = tokenA.balanceOf(bob);

        assertGt(amountOut, 0, "Should get tokens out");
        assertEq(bobAAfter - bobABefore, amountOut, "Balance should increase");

        vm.stopPrank();

        console.log("Swapped 1000 tokenB for tokenA:", amountOut / 1e18);
        console.log("Test PASSED: Swap B for A");
    }

    function testRemoveLiquidity() public {
        // Add liquidity
        vm.startPrank(alice);
        tokenA.approve(address(amm), 10000 * 1e18);
        tokenB.approve(address(amm), 10000 * 1e18);
        uint256 liquidity = amm.addLiquidity(10000 * 1e18, 10000 * 1e18);

        uint256 aliceABefore = tokenA.balanceOf(alice);
        uint256 aliceBBefore = tokenB.balanceOf(alice);

        // Remove all liquidity
        (uint256 amountA, uint256 amountB) = amm.removeLiquidity(liquidity);

        uint256 aliceAAfter = tokenA.balanceOf(alice);
        uint256 aliceBAfter = tokenB.balanceOf(alice);

        assertEq(aliceAAfter - aliceABefore, amountA, "Should get tokenA back");
        assertEq(aliceBAfter - aliceBBefore, amountB, "Should get tokenB back");
        assertEq(amm.liquidity(alice), 0, "Should have 0 liquidity");

        vm.stopPrank();

        console.log("Removed liquidity, got A:", amountA / 1e18);
        console.log("Removed liquidity, got B:", amountB / 1e18);
        console.log("Test PASSED: Remove liquidity");
    }

    function testSlippageProtection() public {
        // Add liquidity
        vm.startPrank(alice);
        tokenA.approve(address(amm), 10000 * 1e18);
        tokenB.approve(address(amm), 10000 * 1e18);
        amm.addLiquidity(10000 * 1e18, 10000 * 1e18);
        vm.stopPrank();

        // Bob tries to swap with high minAmountOut (should fail)
        vm.startPrank(bob);
        tokenA.approve(address(amm), 1000 * 1e18);

        // Expect ~909 tokenB due to slippage, but require 950
        vm.expectRevert("Slippage too high");
        amm.swap(address(tokenA), 1000 * 1e18, 950 * 1e18);

        vm.stopPrank();

        console.log("Test PASSED: Slippage protection works");
    }

    function testPriceImpact() public {
        // Add liquidity
        vm.startPrank(alice);
        tokenA.approve(address(amm), 10000 * 1e18);
        tokenB.approve(address(amm), 10000 * 1e18);
        amm.addLiquidity(10000 * 1e18, 10000 * 1e18);
        vm.stopPrank();

        // Small swap: 100 tokens (1% of pool)
        vm.startPrank(bob);
        tokenA.approve(address(amm), 5000 * 1e18);

        uint256 out1 = amm.swap(address(tokenA), 100 * 1e18, 0);
        console.log("Swap 100 (1% of pool), got:", out1 / 1e18);

        // Larger swap: 1000 tokens (10% of pool) - should have more slippage
        uint256 out2 = amm.swap(address(tokenA), 1000 * 1e18, 0);
        console.log("Swap 1000 (10% of pool), got:", out2 / 1e18);

        // Price per token should be worse for larger trade
        uint256 priceSmall = (out1 * 100) / (100 * 1e18);
        uint256 priceLarge = (out2 * 100) / (1000 * 1e18);

        console.log("Effective rate small swap:", priceSmall, "/ 100");
        console.log("Effective rate large swap:", priceLarge, "/ 100");

        vm.stopPrank();

        console.log("Test PASSED: Demonstrated price impact");
    }
}
