// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/07_Upgradeable.sol";

contract UpgradeableTest is Test {
    SimpleProxy public proxy;
    CounterV1 public implV1;
    CounterV2 public implV2;

    address public admin = makeAddr("admin");
    address public user = makeAddr("user");

    function setUp() public {
        vm.startPrank(admin);

        // Deploy V1 implementation
        implV1 = new CounterV1();

        // Deploy proxy pointing to V1
        proxy = new SimpleProxy(address(implV1));

        vm.stopPrank();
    }

    function testProxyDeployment() public view {
        assertEq(proxy.implementation(), address(implV1), "Implementation should be V1");
        assertEq(proxy.admin(), admin, "Admin should be deployer");

        console.log("Proxy deployed at:", address(proxy));
        console.log("Implementation V1 at:", address(implV1));
        console.log("Test PASSED: Proxy deployment");
    }

    function testDelegatecallWorks() public {
        // Cast proxy as CounterV1
        CounterV1 counter = CounterV1(address(proxy));

        // Call through proxy
        assertEq(counter.getCount(), 0, "Initial count should be 0");

        counter.increment();
        assertEq(counter.getCount(), 1, "Count should be 1");

        counter.increment();
        counter.increment();
        assertEq(counter.getCount(), 3, "Count should be 3");

        console.log("Count after 3 increments:", counter.getCount());
        console.log("Test PASSED: Delegatecall works");
    }

    function testVersionReturnsV1() public view {
        CounterV1 counter = CounterV1(address(proxy));
        string memory version = counter.version();

        assertEq(keccak256(bytes(version)), keccak256(bytes("v1")), "Should be v1");

        console.log("Version:", version);
        console.log("Test PASSED: Version returns v1");
    }

    function testUpgradeToV2() public {
        // Deploy V2
        vm.prank(admin);
        implV2 = new CounterV2();

        // Upgrade
        vm.prank(admin);
        proxy.upgradeTo(address(implV2));

        assertEq(proxy.implementation(), address(implV2), "Should point to V2");

        // Check version changed
        CounterV2 counter = CounterV2(address(proxy));
        assertEq(keccak256(bytes(counter.version())), keccak256(bytes("v2")), "Should be v2");

        console.log("Upgraded to V2 at:", address(implV2));
        console.log("Test PASSED: Upgrade to V2");
    }

    function testStoragePersistsAfterUpgrade() public {
        // Increment in V1
        CounterV1 counterV1 = CounterV1(address(proxy));
        counterV1.increment();
        counterV1.increment();
        counterV1.increment();
        assertEq(counterV1.getCount(), 3, "Count should be 3");

        // Deploy and upgrade to V2
        vm.startPrank(admin);
        implV2 = new CounterV2();
        proxy.upgradeTo(address(implV2));
        vm.stopPrank();

        // Count should persist!
        CounterV2 counterV2 = CounterV2(address(proxy));
        assertEq(counterV2.getCount(), 3, "Count should still be 3 after upgrade");

        console.log("Count before upgrade: 3");
        console.log("Count after upgrade:", counterV2.getCount());
        console.log("Test PASSED: Storage persists after upgrade");
    }

    function testV2NewFeatures() public {
        // Setup: increment in V1, then upgrade
        CounterV1 counterV1 = CounterV1(address(proxy));
        counterV1.increment(); // count = 1

        vm.startPrank(admin);
        implV2 = new CounterV2();
        proxy.upgradeTo(address(implV2));
        vm.stopPrank();

        CounterV2 counterV2 = CounterV2(address(proxy));

        // Test new feature: setIncrementAmount
        counterV2.setIncrementAmount(10);
        counterV2.increment();
        assertEq(counterV2.getCount(), 11, "Should increment by 10");

        // Test new feature: decrement
        counterV2.decrement();
        assertEq(counterV2.getCount(), 10, "Should decrement by 1");
    }

    function testOnlyAdminCanUpgrade() public {
        vm.prank(admin);
        implV2 = new CounterV2();

        // Non-admin tries to upgrade
        vm.prank(user);
        vm.expectRevert("Not admin");
        proxy.upgradeTo(address(implV2));

        console.log("Test PASSED: Only admin can upgrade");
    }

    function testChangeAdmin() public {
        // Admin changes to user
        vm.prank(admin);
        proxy.changeAdmin(user);

        assertEq(proxy.admin(), user, "Admin should be user now");

        // Old admin can't upgrade anymore
        vm.prank(admin);
        implV2 = new CounterV2();

        vm.prank(admin);
        vm.expectRevert("Not admin");
        proxy.upgradeTo(address(implV2));

        // New admin can upgrade
        vm.prank(user);
        proxy.upgradeTo(address(implV2));

        assertEq(proxy.implementation(), address(implV2), "User should have upgraded");

        console.log("Test PASSED: Change admin works");
    }

    function testLastCallerTracked() public {
        CounterV1 counter = CounterV1(address(proxy));

        vm.prank(user);
        counter.increment();

        assertEq(counter.lastCaller(), user, "Last caller should be user");

        vm.prank(admin);
        counter.increment();

        assertEq(counter.lastCaller(), admin, "Last caller should be admin");

        console.log("Test PASSED: lastCaller tracked correctly");
    }

    function testMultipleUpgrades() public {
        CounterV1 counterV1 = CounterV1(address(proxy));
        counterV1.increment(); // count = 1

        // Upgrade to V2
        vm.startPrank(admin);
        implV2 = new CounterV2();
        proxy.upgradeTo(address(implV2));

        CounterV2 counterV2 = CounterV2(address(proxy));
        counterV2.increment(); // count = 2 (default increment)

        // "Upgrade" to a new V2 (same code, fresh deploy)
        CounterV2 implV2New = new CounterV2();
        proxy.upgradeTo(address(implV2New));
        vm.stopPrank();

        // Storage still persists
        assertEq(counterV2.getCount(), 2, "Count should still be 2");

        console.log("Test PASSED: Multiple upgrades work");
    }
}
