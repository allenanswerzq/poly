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

// ============================================
// DEMO / TEST CONTRACT
// Run: forge test --match-contract UpgradeableDemo -vvvv
// ============================================
contract UpgradeableDemo {
    SimpleProxy public proxy;
    CounterV1 public implV1;
    CounterV2 public implV2;

    // Events for logging
    event Log(string message, uint256 value);
    event LogAddress(string message, address value);

    function runAllTests() external {
        testDeployAndIncrement();
        testUpgradeToV2();
        testV2Features();
        testStoragePersistence();
    }

    // Test 1: Deploy proxy with V1 and increment
    function testDeployAndIncrement() public {
        // Deploy implementation V1
        implV1 = new CounterV1();
        emit LogAddress("Deployed CounterV1 at", address(implV1));

        // Deploy proxy pointing to V1
        proxy = new SimpleProxy(address(implV1));
        emit LogAddress("Deployed Proxy at", address(proxy));

        // Cast proxy as CounterV1 to call its functions
        CounterV1 counter = CounterV1(address(proxy));

        // Check initial state
        require(counter.getCount() == 0, "Should start at 0");
        emit Log("Initial count", counter.getCount());

        // Increment
        counter.increment();
        require(counter.getCount() == 1, "Should be 1");
        emit Log("After increment", counter.getCount());

        // Increment again
        counter.increment();
        require(counter.getCount() == 2, "Should be 2");
        emit Log("After 2nd increment", counter.getCount());

        // Check version
        require(
            keccak256(bytes(counter.version())) == keccak256(bytes("v1")),
            "Should be v1"
        );
        emit Log("Test 1 PASSED: Deploy and increment", 1);
    }

    // Test 2: Upgrade to V2
    function testUpgradeToV2() public {
        // Deploy V2 implementation
        implV2 = new CounterV2();
        emit LogAddress("Deployed CounterV2 at", address(implV2));

        // Upgrade proxy to V2
        proxy.upgradeTo(address(implV2));
        emit LogAddress("Upgraded proxy to", address(implV2));

        // Cast as V2
        CounterV2 counter = CounterV2(address(proxy));

        // Check version changed
        require(
            keccak256(bytes(counter.version())) == keccak256(bytes("v2")),
            "Should be v2"
        );
        emit Log("Test 2 PASSED: Upgrade to V2", 1);
    }

    // Test 3: V2 new features work
    function testV2Features() public {
        CounterV2 counter = CounterV2(address(proxy));

        // Get current count (should persist from V1)
        uint256 currentCount = counter.getCount();
        emit Log("Count after upgrade", currentCount);

        // Test new V2 feature: setIncrementAmount
        counter.setIncrementAmount(5);
        counter.increment();
        require(counter.getCount() == currentCount + 5, "Should add 5");
        emit Log("After increment by 5", counter.getCount());

        // Test new V2 feature: decrement
        counter.decrement();
        require(counter.getCount() == currentCount + 4, "Should subtract 1");
        emit Log("After decrement", counter.getCount());

        emit Log("Test 3 PASSED: V2 features work", 1);
    }

    // Test 4: Storage persists across upgrades
    function testStoragePersistence() public {
        CounterV2 counter = CounterV2(address(proxy));

        // Set a specific count
        counter.setIncrementAmount(100);
        counter.increment();
        uint256 countBefore = counter.getCount();
        emit Log("Count before re-upgrade", countBefore);

        // "Upgrade" back to same V2 (simulates upgrade)
        proxy.upgradeTo(address(implV2));

        // Count should still be there!
        require(counter.getCount() == countBefore, "Storage should persist");
        emit Log("Count after re-upgrade", counter.getCount());

        emit Log("Test 4 PASSED: Storage persists", 1);
    }

    // Demonstrate the delegatecall magic
    function explainDelegatecall() external view returns (string memory) {
        return string(abi.encodePacked(
            "When you call proxy.increment():\n",
            "1. Proxy receives the call\n",
            "2. fallback() catches it (no increment function in proxy)\n",
            "3. delegatecall forwards to implementation\n",
            "4. Implementation code runs\n",
            "5. But storage writes go to PROXY, not implementation!\n",
            "6. Result: upgradeable logic, persistent storage"
        ));
    }
}

// ============================================
// HOW TO RUN THIS DEMO
// ============================================
//
// Option 1: In Foundry test
//   forge test --match-contract UpgradeableDemo -vvvv
//
// Option 2: Deploy and call manually
//   UpgradeableDemo demo = new UpgradeableDemo();
//   demo.runAllTests();
//
// Option 3: In Remix
//   Deploy UpgradeableDemo
//   Click runAllTests()
//   Check the event logs
// ============================================
