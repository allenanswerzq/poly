// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 9: SIGNATURES & EIP-712
 *
 * Learn how to verify off-chain signatures on-chain.
 * This enables gasless transactions, meta-transactions, and permits.
 */

contract SignatureVerifier {
    // EIP-712 Domain Separator
    bytes32 public immutable DOMAIN_SEPARATOR;

    // Type hashes for structured data
    bytes32 public constant TRANSFER_TYPEHASH =
        keccak256("Transfer(address from,address to,uint256 amount,uint256 nonce,uint256 deadline)");

    // Nonces for replay protection
    mapping(address => uint256) public nonces;

    // Example balance tracking
    mapping(address => uint256) public balances;

    event TransferWithSignature(address indexed from, address indexed to, uint256 amount);

    constructor() {
        DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256(bytes("SignatureVerifier")),
                keccak256(bytes("1")),
                block.chainid,
                address(this)
            )
        );
    }

    /**
     * TRANSFER WITH SIGNATURE
     *
     * Anyone can submit this transaction, but it requires
     * a valid signature from the `from` address.
     *
     * This enables:
     * - Gasless transactions (relayer pays gas)
     * - Batch operations
     * - Better UX
     */
    function transferWithSignature(
        address from,
        address to,
        uint256 amount,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external {
        require(block.timestamp <= deadline, "Signature expired");

        // Build the struct hash
        bytes32 structHash = keccak256(
            abi.encode(
                TRANSFER_TYPEHASH,
                from,
                to,
                amount,
                nonces[from]++,  // Use and increment nonce
                deadline
            )
        );

        // Build the digest (what was actually signed)
        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                DOMAIN_SEPARATOR,
                structHash
            )
        );

        // Recover signer
        address signer = ecrecover(digest, v, r, s);
        require(signer != address(0) && signer == from, "Invalid signature");

        // Execute transfer
        require(balances[from] >= amount, "Insufficient balance");
        balances[from] -= amount;
        balances[to] += amount;

        emit TransferWithSignature(from, to, amount);
    }

    /**
     * SIMPLE SIGNATURE VERIFICATION
     *
     * For simpler cases (not EIP-712)
     */
    function verifySimpleSignature(
        bytes32 messageHash,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external pure returns (address) {
        // Ethereum signed message prefix
        bytes32 ethSignedHash = keccak256(
            abi.encodePacked("\x19Ethereum Signed Message:\n32", messageHash)
        );

        return ecrecover(ethSignedHash, v, r, s);
    }

    /**
     * GET DIGEST FOR SIGNING
     *
     * Off-chain, you'd sign this digest
     */
    function getTransferDigest(
        address from,
        address to,
        uint256 amount,
        uint256 deadline
    ) external view returns (bytes32) {
        bytes32 structHash = keccak256(
            abi.encode(
                TRANSFER_TYPEHASH,
                from,
                to,
                amount,
                nonces[from],
                deadline
            )
        );

        return keccak256(
            abi.encodePacked(
                "\x19\x01",
                DOMAIN_SEPARATOR,
                structHash
            )
        );
    }

    // Helper for testing
    function deposit() external payable {
        balances[msg.sender] += msg.value;
    }
}

/**
 * ERC20 PERMIT (EIP-2612)
 *
 * The most common use of EIP-712 signatures.
 * Allows approval + transfer in single transaction.
 */
contract ERC20WithPermit {
    string public name = "PermitToken";
    string public symbol = "PRMT";
    uint8 public decimals = 18;
    uint256 public totalSupply;

    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    mapping(address => uint256) public nonces;

    bytes32 public immutable DOMAIN_SEPARATOR;
    bytes32 public constant PERMIT_TYPEHASH =
        keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");

    constructor() {
        DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256(bytes(name)),
                keccak256(bytes("1")),
                block.chainid,
                address(this)
            )
        );
    }

    /**
     * PERMIT: Approve with signature
     *
     * Instead of:
     *   tx1: token.approve(spender, amount)
     *   tx2: spender.doSomething()
     *
     * Now:
     *   signature = sign(permit)
     *   tx1: spender.doSomethingWithPermit(signature)
     *
     * ONE transaction, better UX!
     */
    function permit(
        address owner,
        address spender,
        uint256 value,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external {
        require(block.timestamp <= deadline, "Permit expired");

        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                DOMAIN_SEPARATOR,
                keccak256(
                    abi.encode(
                        PERMIT_TYPEHASH,
                        owner,
                        spender,
                        value,
                        nonces[owner]++,
                        deadline
                    )
                )
            )
        );

        address recoveredAddress = ecrecover(digest, v, r, s);
        require(recoveredAddress != address(0) && recoveredAddress == owner, "Invalid signature");

        allowance[owner][spender] = value;
    }

    // Standard ERC20 functions
    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        if (allowed != type(uint256).max) {
            allowance[from][msg.sender] = allowed - amount;
        }
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        return true;
    }

    function mint(address to, uint256 amount) external {
        totalSupply += amount;
        balanceOf[to] += amount;
    }
}

/**
 * CONCEPTS:
 *
 * 1. WHY EIP-712?
 *    - Structured data signing (not just raw bytes)
 *    - Users see what they're signing in wallet
 *    - Prevents replay across chains/contracts
 *
 * 2. DOMAIN SEPARATOR
 *    - Unique to each contract deployment
 *    - Contains: name, version, chainId, contract address
 *    - Prevents cross-contract replay
 *
 * 3. NONCES
 *    - Each signature can only be used once
 *    - Incremented after each use
 *    - Prevents replay of same signature
 *
 * 4. DEADLINE
 *    - Signatures expire after deadline
 *    - Prevents using old signatures
 *
 * 5. ECRECOVER
 *    - Recovers signer address from signature
 *    - Returns zero on invalid signature
 *    - Always check for zero address!
 */
