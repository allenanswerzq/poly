// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * EXERCISE 7: UPGRADEABLE CONTRACTS (Proxy Pattern)
 *
 * Learn how to build upgradeable smart contracts.
 * This is how protocols like Aave, Compound, OpenSea work.
 */

/**
 * PROXY CONTRACT
 *
 * This is the contract users interact with.
 * It DELEGATES all calls to the implementation.
 * Storage lives here, logic lives in implementation.
 */
contract SimpleProxy {
    // EIP-1967 storage slots (random to avoid collision)
    bytes32 private constant IMPLEMENTATION_SLOT =
        0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;
    bytes32 private constant ADMIN_SLOT =
        0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103;

    constructor(address _implementation) {
        _setAdmin(msg.sender);
        _setImplementation(_implementation);
    }

    /**
     * DELEGATECALL: The magic behind proxies
     *
     * - Executes implementation's code
     * - But uses PROXY's storage
     * - msg.sender and msg.value are preserved
     */
    fallback() external payable {
        address impl = _getImplementation();
        require(impl != address(0), "No implementation");

        assembly {
            // Copy calldata to memory
            calldatacopy(0, 0, calldatasize())

            // Delegatecall to implementation
            let result := delegatecall(gas(), impl, 0, calldatasize(), 0, 0)

            // Copy return data
            returndatacopy(0, 0, returndatasize())

            // Return or revert based on result
            switch result
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }

    receive() external payable {}

    // Admin functions
    function upgradeTo(address newImplementation) external {
        require(msg.sender == _getAdmin(), "Not admin");
        _setImplementation(newImplementation);
    }

    function changeAdmin(address newAdmin) external {
        require(msg.sender == _getAdmin(), "Not admin");
        _setAdmin(newAdmin);
    }

    // Storage helpers using EIP-1967 slots
    function _getImplementation() internal view returns (address impl) {
        bytes32 slot = IMPLEMENTATION_SLOT;
        assembly {
            impl := sload(slot)
        }
    }

    function _setImplementation(address impl) internal {
        bytes32 slot = IMPLEMENTATION_SLOT;
        assembly {
            sstore(slot, impl)
        }
    }

    function _getAdmin() internal view returns (address admin) {
        bytes32 slot = ADMIN_SLOT;
        assembly {
            admin := sload(slot)
        }
    }

    function _setAdmin(address admin) internal {
        bytes32 slot = ADMIN_SLOT;
        assembly {
            sstore(slot, admin)
        }
    }

    // View functions for testing
    function implementation() external view returns (address) {
        return _getImplementation();
    }

    function admin() external view returns (address) {
        return _getAdmin();
    }
}

/**
 * IMPLEMENTATION V1
 *
 * First version of the logic contract.
 */
contract CounterV1 {
    // Storage layout must match across versions!
    uint256 public count;
    address public lastCaller;

    function increment() external {
        count += 1;
        lastCaller = msg.sender;
    }

    function getCount() external view returns (uint256) {
        return count;
    }

    function version() external pure returns (string memory) {
        return "v1";
    }
}

/**
 * IMPLEMENTATION V2
 *
 * Upgraded version with new features.
 * IMPORTANT: Cannot change existing storage layout!
 */
contract CounterV2 {
    // Same storage layout as V1
    uint256 public count;
    address public lastCaller;

    // New storage variables must be APPENDED
    uint256 public incrementAmount;

    function increment() external {
        // V2 feature: configurable increment
        uint256 amount = incrementAmount > 0 ? incrementAmount : 1;
        count += amount;
        lastCaller = msg.sender;
    }

    function setIncrementAmount(uint256 _amount) external {
        incrementAmount = _amount;
    }

    function decrement() external {
        // V2 feature: new function
        require(count > 0, "Already zero");
        count -= 1;
        lastCaller = msg.sender;
    }

    function getCount() external view returns (uint256) {
        return count;
    }

    function version() external pure returns (string memory) {
        return "v2";
    }
}

/**
 * STORAGE COLLISION EXAMPLE - DON'T DO THIS!
 */
contract BadV2 {
    // WRONG! Changed storage layout
    address public lastCaller;  // Was slot 1, now slot 0!
    uint256 public count;       // Was slot 0, now slot 1!

    // This will read/write wrong data!
}

/**
 * KEY CONCEPTS:
 *
 * 1. DELEGATECALL
 *    - Executes code in context of caller
 *    - Storage, msg.sender, msg.value from proxy
 *    - Code from implementation
 *
 * 2. STORAGE LAYOUT
 *    - Must be identical across versions
 *    - New variables: APPEND ONLY
 *    - Never delete or reorder existing variables
 *
 * 3. EIP-1967 SLOTS
 *    - Use random slots for proxy admin data
 *    - Avoids collision with implementation storage
 *
 * 4. INITIALIZATION
 *    - Can't use constructor (runs on implementation)
 *    - Use initializer function instead
 *    - Must protect against re-initialization
 *
 * 5. SECURITY RISKS
 *    - Admin can upgrade to malicious code
 *    - Storage collision bugs
 *    - Initialization front-running
 */
