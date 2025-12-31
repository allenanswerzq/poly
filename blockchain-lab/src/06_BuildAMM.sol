// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "./tokens/ERC20.sol";

/**
 * EXERCISE 6: BUILD YOUR OWN AMM (Automated Market Maker)
 *
 * This is a simplified Uniswap V2 clone. You'll learn:
 * - Constant product formula (x * y = k)
 * - Liquidity provision
 * - Price discovery
 * - Slippage
 */

contract SimpleAMM {
    // The two tokens in this pool
    IERC20 public immutable tokenA;
    IERC20 public immutable tokenB;

    // Reserves
    uint256 public reserveA;
    uint256 public reserveB;

    // LP tokens (track liquidity providers)
    uint256 public totalLiquidity;
    mapping(address => uint256) public liquidity;

    // Events
    event LiquidityAdded(address indexed provider, uint256 amountA, uint256 amountB, uint256 liquidity);
    event LiquidityRemoved(address indexed provider, uint256 amountA, uint256 amountB, uint256 liquidity);
    event Swap(address indexed user, address tokenIn, uint256 amountIn, uint256 amountOut);

    constructor(address _tokenA, address _tokenB) {
        tokenA = IERC20(_tokenA);
        tokenB = IERC20(_tokenB);
    }

    /**
     * ADD LIQUIDITY
     *
     * First provider sets the initial ratio.
     * Subsequent providers must match the current ratio.
     */
    function addLiquidity(uint256 amountA, uint256 amountB) external returns (uint256 liquidityMinted) {
        // Transfer tokens from user
        tokenA.transferFrom(msg.sender, address(this), amountA);
        tokenB.transferFrom(msg.sender, address(this), amountB);

        if (totalLiquidity == 0) {
            // First liquidity provider - set initial ratio
            // LP tokens = sqrt(amountA * amountB)
            liquidityMinted = sqrt(amountA * amountB);
        } else {
            // Subsequent providers must match ratio
            // LP tokens proportional to contribution
            uint256 liquidityA = (amountA * totalLiquidity) / reserveA;
            uint256 liquidityB = (amountB * totalLiquidity) / reserveB;
            liquidityMinted = liquidityA < liquidityB ? liquidityA : liquidityB;
        }

        require(liquidityMinted > 0, "Insufficient liquidity minted");

        // Update state
        liquidity[msg.sender] += liquidityMinted;
        totalLiquidity += liquidityMinted;
        reserveA += amountA;
        reserveB += amountB;

        emit LiquidityAdded(msg.sender, amountA, amountB, liquidityMinted);
    }

    /**
     * REMOVE LIQUIDITY
     *
     * Burn LP tokens, receive proportional share of both tokens.
     */
    function removeLiquidity(uint256 liquidityAmount) external returns (uint256 amountA, uint256 amountB) {
        require(liquidity[msg.sender] >= liquidityAmount, "Insufficient liquidity");

        // Calculate proportional amounts
        amountA = (liquidityAmount * reserveA) / totalLiquidity;
        amountB = (liquidityAmount * reserveB) / totalLiquidity;

        // Update state
        liquidity[msg.sender] -= liquidityAmount;
        totalLiquidity -= liquidityAmount;
        reserveA -= amountA;
        reserveB -= amountB;

        // Transfer tokens
        tokenA.transfer(msg.sender, amountA);
        tokenB.transfer(msg.sender, amountB);

        emit LiquidityRemoved(msg.sender, amountA, amountB, liquidityAmount);
    }

    /**
     * SWAP: The core AMM function!
     *
     * Uses constant product formula: x * y = k
     * After swap: (x + dx) * (y - dy) = k
     * Solving for dy: dy = (y * dx) / (x + dx)
     */
    function swap(address tokenIn, uint256 amountIn, uint256 minAmountOut) external returns (uint256 amountOut) {
        require(tokenIn == address(tokenA) || tokenIn == address(tokenB), "Invalid token");
        require(amountIn > 0, "Amount must be > 0");

        bool isTokenA = tokenIn == address(tokenA);

        // Get current reserves
        (uint256 reserveIn, uint256 reserveOut) = isTokenA
            ? (reserveA, reserveB)
            : (reserveB, reserveA);

        // Transfer input token
        IERC20(tokenIn).transferFrom(msg.sender, address(this), amountIn);

        // Calculate output using constant product formula
        // dy = (y * dx) / (x + dx)
        // With 0.3% fee: dx_fee = dx * 997 / 1000
        uint256 amountInWithFee = amountIn * 997;
        amountOut = (reserveOut * amountInWithFee) / (reserveIn * 1000 + amountInWithFee);

        require(amountOut >= minAmountOut, "Slippage too high");
        require(amountOut < reserveOut, "Insufficient liquidity");

        // Update reserves
        if (isTokenA) {
            reserveA += amountIn;
            reserveB -= amountOut;
            tokenB.transfer(msg.sender, amountOut);
        } else {
            reserveB += amountIn;
            reserveA -= amountOut;
            tokenA.transfer(msg.sender, amountOut);
        }

        emit Swap(msg.sender, tokenIn, amountIn, amountOut);
    }

    /**
     * GET PRICE
     *
     * Returns how many tokenB you get for 1 tokenA
     */
    function getPrice() external view returns (uint256) {
        require(reserveA > 0, "No liquidity");
        return (reserveB * 1e18) / reserveA;
    }

    /**
     * QUOTE: Calculate expected output for a given input
     */
    function getAmountOut(address tokenIn, uint256 amountIn) external view returns (uint256) {
        bool isTokenA = tokenIn == address(tokenA);
        (uint256 reserveIn, uint256 reserveOut) = isTokenA
            ? (reserveA, reserveB)
            : (reserveB, reserveA);

        uint256 amountInWithFee = amountIn * 997;
        return (reserveOut * amountInWithFee) / (reserveIn * 1000 + amountInWithFee);
    }

    /**
     * HELPER: Square root (Babylonian method)
     */
    function sqrt(uint256 x) internal pure returns (uint256 y) {
        if (x == 0) return 0;
        uint256 z = (x + 1) / 2;
        y = x;
        while (z < y) {
            y = z;
            z = (x / z + z) / 2;
        }
    }
}

/**
 * CONCEPTS TO UNDERSTAND:
 *
 * 1. CONSTANT PRODUCT (x * y = k)
 *    - The product of reserves is always constant
 *    - As one goes up, the other must go down
 *    - This creates the price curve
 *
 * 2. SLIPPAGE
 *    - Larger trades move the price more
 *    - Trading 1% of pool: ~1% slippage
 *    - Trading 10% of pool: ~10% slippage
 *
 * 3. IMPERMANENT LOSS
 *    - LPs lose value when prices diverge from entry
 *    - Called "impermanent" because it reverses if price returns
 *
 * 4. LP TOKENS
 *    - Represent your share of the pool
 *    - Increase in value from trading fees
 */
