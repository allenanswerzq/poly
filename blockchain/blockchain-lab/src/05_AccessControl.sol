// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 5: ACCESS CONTROL VULNERABILITIES
 *
 * Common mistakes in access control that lead to exploits.
 */

// ============================================
// VULNERABILITY 1: Missing Access Control
// ============================================
contract MissingAccessControl {
    address public owner;
    bool public paused;

    constructor() {
        owner = msg.sender;
    }

    // VULNERABLE: Anyone can call this!
    function setOwner(address newOwner) external {
        owner = newOwner;  // Missing onlyOwner check!
    }

    // VULNERABLE: Anyone can pause!
    function pause() external {
        paused = true;
    }

    // VULNERABLE: Anyone can mint!
    function mint(address to, uint256 amount) external {
        // _mint(to, amount);  // Anyone can create tokens!
    }
}

// ============================================
// VULNERABILITY 2: tx.origin Authentication
// ============================================
contract TxOriginPhishing {
    address public owner;

    constructor() {
        owner = msg.sender;
    }

    // VULNERABLE: Uses tx.origin instead of msg.sender
    function transferOwnership(address newOwner) external {
        require(tx.origin == owner, "Not owner");  // BAD!
        owner = newOwner;
    }

    function withdraw() external {
        require(tx.origin == owner, "Not owner");  // BAD!
        payable(msg.sender).transfer(address(this).balance);
    }

    receive() external payable {}
}

// Attacker contract
contract TxOriginAttacker {
    TxOriginPhishing public victim;
    address public attacker;

    constructor(address _victim) {
        victim = TxOriginPhishing(payable(_victim));
        attacker = msg.sender;
    }

    // Trick: Get owner to call this (disguised as something else)
    function claimAirdrop() external {
        // When owner calls this, tx.origin = owner
        // So victim.withdraw() will succeed!
        victim.withdraw();
        payable(attacker).transfer(address(this).balance);
    }

    receive() external payable {}
}

// ============================================
// VULNERABILITY 3: Unprotected Initialize
// ============================================
contract UnprotectedInitialize {
    address public owner;
    bool public initialized;

    // VULNERABLE: Anyone can call initialize!
    function initialize(address _owner) external {
        require(!initialized, "Already initialized");
        owner = _owner;
        initialized = true;
    }

    // In a proxy pattern, attacker can front-run initialize()
    // and become owner!
}

// ============================================
// VULNERABILITY 4: Signature Replay
// ============================================
contract SignatureReplay {
    mapping(address => uint256) public nonces;
    mapping(address => uint256) public balances;

    // VULNERABLE: No nonce, signature can be replayed!
    function withdrawWithSignature_VULNERABLE(
        address to,
        uint256 amount,
        bytes calldata signature
    ) external {
        bytes32 message = keccak256(abi.encodePacked(to, amount));
        address signer = recoverSigner(message, signature);

        require(balances[signer] >= amount, "Insufficient balance");
        balances[signer] -= amount;
        payable(to).transfer(amount);

        // Attacker can submit same signature again!
    }

    // FIXED: Include nonce to prevent replay
    function withdrawWithSignature_FIXED(
        address to,
        uint256 amount,
        uint256 nonce,
        bytes calldata signature
    ) external {
        require(nonce == nonces[to], "Invalid nonce");

        bytes32 message = keccak256(abi.encodePacked(to, amount, nonce));
        address signer = recoverSigner(message, signature);

        require(balances[signer] >= amount, "Insufficient balance");

        nonces[to]++;  // Increment nonce
        balances[signer] -= amount;
        payable(to).transfer(amount);
    }

    function recoverSigner(bytes32 message, bytes calldata sig) internal pure returns (address) {
        require(sig.length == 65, "Invalid signature length");

        bytes32 r;
        bytes32 s;
        uint8 v;

        assembly {
            r := calldataload(sig.offset)
            s := calldataload(add(sig.offset, 32))
            v := byte(0, calldataload(add(sig.offset, 64)))
        }

        return ecrecover(keccak256(abi.encodePacked("\x19Ethereum Signed Message:\n32", message)), v, r, s);
    }

    receive() external payable {}
}

// ============================================
// FIXED: Proper Access Control
// ============================================
contract SecureAccessControl {
    address public owner;
    mapping(address => bool) public admins;
    bool public initialized;

    event OwnershipTransferred(address indexed oldOwner, address indexed newOwner);

    modifier onlyOwner() {
        require(msg.sender == owner, "Not owner");
        _;
    }

    modifier onlyAdmin() {
        require(admins[msg.sender] || msg.sender == owner, "Not admin");
        _;
    }

    constructor() {
        owner = msg.sender;
        initialized = true;  // Set in constructor, not initializer
    }

    // FIXED: Proper access control
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "Zero address");
        emit OwnershipTransferred(owner, newOwner);
        owner = newOwner;
    }

    function addAdmin(address admin) external onlyOwner {
        admins[admin] = true;
    }

    function removeAdmin(address admin) external onlyOwner {
        admins[admin] = false;
    }

    // For proxy pattern: use initializer guard
    modifier initializer() {
        require(!initialized, "Already initialized");
        _;
        initialized = true;
    }
}

// ============================================
// BEST PRACTICE: OpenZeppelin Access Control
// ============================================
// import "@openzeppelin/contracts/access/Ownable.sol";
// import "@openzeppelin/contracts/access/AccessControl.sol";
//
// contract MyContract is Ownable, AccessControl {
//     bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");
//     bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
//
//     function mint(address to, uint256 amount) external onlyRole(MINTER_ROLE) {
//         _mint(to, amount);
//     }
// }
