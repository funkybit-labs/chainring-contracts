// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {Exchange} from "../src/Exchange.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./utils/SigUtils.sol";
import {ExchangeBaseTest} from "./ExchangeBaseTest.sol";
import {MockERC20} from "./contracts/MockERC20.sol";
import "../src/interfaces/IExchange.sol";
import "../src/interfaces/IExchange.sol";
import "../src/interfaces/IExchange.sol";

contract SovereignWithdrawalTest is ExchangeBaseTest {
    function setUp() public override {
        super.setUp();
        vm.deal(wallet1, 10 ether);
        vm.deal(wallet2, 10 ether);
    }

    function test_sovereignWithdrawal_initiateNew_nativeToken() public {
        setupWallets();
        deposit(wallet1, 2 ether);
        vm.startPrank(wallet1);

        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.WithdrawalRequested(address(wallet1), address(0), 2 ether);
        exchange.sovereignWithdrawal(address(0), 2 ether);

        (address tokenAddress, uint256 amount, uint256 timestamp) = exchange.sovereignWithdrawals(wallet1);
        assertEq(tokenAddress, address(0));
        assertEq(amount, 2 ether);
        assertGt(timestamp, 0);

        vm.stopPrank();
    }

    function test_sovereignWithdrawal_revertIfInsufficientBalance_nativeToken() public {
        setupWallets();
        deposit(wallet1, 3 ether);
        vm.startPrank(wallet1);

        vm.expectRevert("Insufficient balance");
        exchange.sovereignWithdrawal(address(0), 4 ether);

        vm.stopPrank();
    }

    function test_sovereignWithdrawal_revertIfWithdrawalDelayNotMet_nativeToken() public {
        setupWallets();
        deposit(wallet1, 3 ether);
        vm.startPrank(wallet1);

        exchange.sovereignWithdrawal(address(0), 2 ether);

        vm.expectRevert("Withdrawal delay not met");
        exchange.sovereignWithdrawal(address(0), 2 ether);

        vm.stopPrank();
    }

    function test_sovereignWithdrawal_complete_nativeToken() public {
        setupWallets();
        deposit(wallet1, 3 ether);
        vm.startPrank(wallet1);

        exchange.sovereignWithdrawal(address(0), 2 ether);

        // increase EVM time to pass the delay
        vm.warp(block.timestamp + exchange.sovereignWithdrawalDelay());

        exchange.sovereignWithdrawal(address(0), 2 ether);

        assertEq(wallet1.balance, 9 ether); // initial balance - deposit + withdrawal
        (, uint256 amount,) = exchange.sovereignWithdrawals(wallet1);
        assertEq(amount, 0);

        vm.stopPrank();
    }

    function test_sovereignWithdrawalAll_complete_nativeToken() public {
        setupWallets();
        deposit(wallet1, 3 ether);
        vm.startPrank(wallet1);

        exchange.sovereignWithdrawal(address(0), 0 ether);

        // increase EVM time to pass the delay
        vm.warp(block.timestamp + exchange.sovereignWithdrawalDelay());

        exchange.sovereignWithdrawal(address(0), 0 ether);

        assertEq(wallet1.balance, 10 ether); // initial balance - deposit + withdrawal
        (, uint256 amount,) = exchange.sovereignWithdrawals(wallet1);
        assertEq(amount, 0);

        vm.stopPrank();
    }

    function test_sovereignWithdrawal_initiateNewAfterDelayIfAmountDoesNotMatch_nativeToken() public {
        setupWallets();
        deposit(wallet1, 5 ether);
        vm.startPrank(wallet1);

        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.WithdrawalRequested(address(wallet1), address(0), 2 ether);
        exchange.sovereignWithdrawal(address(0), 2 ether);

        // increase EVM time to pass the delay
        vm.warp(block.timestamp + exchange.sovereignWithdrawalDelay());

        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.WithdrawalRequested(address(wallet1), address(0), 4 ether);
        // re-initiate due to amount missmatch
        exchange.sovereignWithdrawal(address(0), 4 ether);

        (address tokenAddress, uint256 amount, uint256 timestamp) = exchange.sovereignWithdrawals(wallet1);
        assertEq(tokenAddress, address(0));
        assertEq(amount, 4 ether);
        assertGt(timestamp, 0);

        vm.stopPrank();
    }

    function test_sovereignWithdrawal_initiateNewAfterDelayIfTokenDoesNotMatch_nativeToken() public {
        setupWallets();
        deposit(wallet1, 5 ether);
        deposit(wallet1, usdcAddress, 1000e6);
        vm.startPrank(wallet1);

        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.WithdrawalRequested(address(wallet1), address(0), 2 ether);
        exchange.sovereignWithdrawal(address(0), 2 ether);

        // increase EVM time to pass the delay
        vm.warp(block.timestamp + exchange.sovereignWithdrawalDelay());

        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.WithdrawalRequested(address(wallet1), usdcAddress, 1000e5);
        // re-initiate due to token missmatch
        exchange.sovereignWithdrawal(usdcAddress, 1000e5);

        (address tokenAddress, uint256 amount, uint256 timestamp) = exchange.sovereignWithdrawals(wallet1);
        assertEq(tokenAddress, usdcAddress);
        assertEq(amount, 1000e5);
        assertGt(timestamp, 0);

        vm.stopPrank();
    }

    function test_setSovereignWithdrawalDelay() public {
        vm.startPrank(exchange.owner());

        exchange.setSovereignWithdrawalDelay(2 days);
        assertEq(exchange.sovereignWithdrawalDelay(), 2 days);

        vm.stopPrank();
    }

    function test_setSovereignWithdrawalDelay_revertIfNotOwner() public {
        vm.startPrank(wallet1);

        vm.expectRevert(abi.encodeWithSelector(OwnableUnauthorizedAccount.selector, wallet1));
        exchange.setSovereignWithdrawalDelay(2 days);

        vm.stopPrank();
    }

    function test_setSovereignWithdrawalDelay_revertIfInvalidDelay() public {
        vm.startPrank(exchange.owner());

        vm.expectRevert(bytes("Not a valid sovereign withdrawal delay"));
        exchange.setSovereignWithdrawalDelay(0);

        vm.expectRevert(bytes("Not a valid sovereign withdrawal delay"));
        exchange.setSovereignWithdrawalDelay(1 seconds);

        vm.expectRevert(bytes("Not a valid sovereign withdrawal delay"));
        exchange.setSovereignWithdrawalDelay(23 hours);

        exchange.setSovereignWithdrawalDelay(1 days);

        vm.stopPrank();
    }
}
