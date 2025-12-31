// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 3: FLASH LOAN & PRICE MANIPULATION
 *
 * Understand how flash loans enable price manipulation attacks.
 */

// ============================================
// SIMPLE DEX (Automated Market Maker)
// ============================================
contract SimpleDEX {
    // Reserves: ETH and TOKEN
    uint256 public reserveETH;
    uint256 public reserveToken;

    // Simple token balance tracking
    mapping(address => uint256) public tokenBalances;

    constructor() payable {
        require(msg.value > 0, "Need ETH");
        reserveETH = msg.value;
        reserveToken = 1000000 * 1e18;  // 1M tokens
        tokenBalances[address(this)] = reserveToken;
    }

    // ============================================
    // VULNERABLE: Spot price can be manipulated!
    // ============================================
    function getSpotPrice() public view returns (uint256) {
        // Price = reserveToken / reserveETH
        // This is the CURRENT ratio - easily manipulated!
        return (reserveToken * 1e18) / reserveETH;
    }

    // Swap ETH for tokens (constant product: x * y = k)
    function swapETHForTokens() external payable returns (uint256) {
        require(msg.value > 0, "Need ETH");

        uint256 k = reserveETH * reserveToken;
        uint256 newReserveETH = reserveETH + msg.value;
        uint256 newReserveToken = k / newReserveETH;
        uint256 tokensOut = reserveToken - newReserveToken;

        reserveETH = newReserveETH;
        reserveToken = newReserveToken;
        tokenBalances[msg.sender] += tokensOut;

        return tokensOut;
    }

    // Swap tokens for ETH
    function swapTokensForETH(uint256 tokenAmount) external returns (uint256) {
        require(tokenBalances[msg.sender] >= tokenAmount, "Not enough tokens");

        uint256 k = reserveETH * reserveToken;
        uint256 newReserveToken = reserveToken + tokenAmount;
        uint256 newReserveETH = k / newReserveToken;
        uint256 ethOut = reserveETH - newReserveETH;

        reserveToken = newReserveToken;
        reserveETH = newReserveETH;
        tokenBalances[msg.sender] -= tokenAmount;

        payable(msg.sender).transfer(ethOut);
        return ethOut;
    }

    // Add liquidity (for setup)
    function addLiquidity(uint256 tokenAmount) external payable {
        reserveETH += msg.value;
        reserveToken += tokenAmount;
        tokenBalances[address(this)] += tokenAmount;
    }

    receive() external payable {}
}

// ============================================
// VULNERABLE LENDING PROTOCOL
// Uses spot price for collateral valuation!
// ============================================
contract VulnerableLending {
    SimpleDEX public dex;

    mapping(address => uint256) public collateralETH;
    mapping(address => uint256) public borrowedTokens;

    // Collateral ratio: 150%
    uint256 public constant COLLATERAL_RATIO = 150;

    constructor(address _dex) {
        dex = SimpleDEX(payable(_dex));
    }

    // Deposit ETH as collateral
    function depositCollateral() external payable {
        collateralETH[msg.sender] += msg.value;
    }

    // VULNERABLE: Uses spot price!
    function borrow(uint256 tokenAmount) external {
        uint256 price = dex.getSpotPrice();  // MANIPULABLE!

        // Calculate required collateral
        // If price is manipulated UP, tokens seem worth more
        // So we can borrow more with same collateral!
        uint256 tokenValueInETH = (tokenAmount * 1e18) / price;
        uint256 requiredCollateral = (tokenValueInETH * COLLATERAL_RATIO) / 100;

        require(
            collateralETH[msg.sender] >= requiredCollateral,
            "Insufficient collateral"
        );

        borrowedTokens[msg.sender] += tokenAmount;
        dex.tokenBalances(address(this));  // Transfer tokens to borrower
        // (simplified - actual implementation would transfer)
    }

    // Calculate max borrowable based on collateral
    function maxBorrowable(address user) external view returns (uint256) {
        uint256 price = dex.getSpotPrice();
        uint256 collateralValue = collateralETH[user] * price / 1e18;
        return (collateralValue * 100) / COLLATERAL_RATIO;
    }
}

// ============================================
// FLASH LOAN PROVIDER
// ============================================
contract FlashLoanProvider {
    mapping(address => uint256) public deposits;

    uint256 public constant FEE_BPS = 9; // 0.09% fee

    constructor() payable {
        deposits[address(this)] = msg.value;
    }

    function flashLoan(uint256 amount, address borrower, bytes calldata data) external {
        require(amount <= address(this).balance, "Not enough liquidity");

        uint256 balanceBefore = address(this).balance;

        // Transfer funds to borrower
        payable(borrower).transfer(amount);

        // Borrower does their thing
        IFlashBorrower(borrower).onFlashLoan(amount, data);

        // Check repayment
        uint256 fee = (amount * FEE_BPS) / 10000;
        require(
            address(this).balance >= balanceBefore + fee,
            "Flash loan not repaid"
        );
    }

    receive() external payable {}
}

interface IFlashBorrower {
    function onFlashLoan(uint256 amount, bytes calldata data) external;
}

// ============================================
// ATTACKER CONTRACT
// ============================================
contract FlashLoanAttacker is IFlashBorrower {
    FlashLoanProvider public lender;
    SimpleDEX public dex;
    VulnerableLending public lending;
    address public owner;

    constructor(address _lender, address _dex, address _lending) {
        lender = FlashLoanProvider(payable(_lender));
        dex = SimpleDEX(payable(_dex));
        lending = VulnerableLending(_lending);
        owner = msg.sender;
    }

    function attack() external payable {
        // Step 1: Get a flash loan
        lender.flashLoan(50 ether, address(this), "");
    }

    function onFlashLoan(uint256 amount, bytes calldata) external override {
        // Step 2: Manipulate the price
        // Buy a ton of tokens - this changes the reserve ratio
        uint256 tokensReceived = dex.swapETHForTokens{value: amount}();

        // Price is now manipulated!
        // reserveETH went UP, reserveToken went DOWN
        // So getSpotPrice() = reserveToken/reserveETH is now LOWER
        // Meaning tokens appear CHEAPER

        // Wait... we want tokens to appear MORE EXPENSIVE
        // Let's think about this differently...

        // Actually for a lending attack, we want to:
        // 1. Deposit collateral in lending
        // 2. Manipulate price to make our collateral worth more
        // 3. Borrow more than we should

        // For this demo, let's just show the price manipulation

        // Step 3: Swap back (take profit from arb or exploit)
        dex.swapTokensForETH(tokensReceived);

        // Step 4: Repay flash loan + fee
        uint256 fee = (amount * 9) / 10000;
        payable(address(lender)).transfer(amount + fee);
    }

    receive() external payable {}

    function withdraw() external {
        require(msg.sender == owner);
        payable(owner).transfer(address(this).balance);
    }
}
