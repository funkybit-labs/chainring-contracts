// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {ERC20Mock} from "openzeppelin-contracts/contracts/mocks/token/ERC20Mock.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {Exchange} from "../src/Exchange.sol";
import "openzeppelin-contracts/contracts/mocks/token/ERC20Mock.sol";


contract ExchangeTest is Test {
    Exchange internal exchange;
    address internal exchangeAddress;
    address internal wallet1;
    address internal wallet2;

    function setUp() public {
        exchange = new Exchange();
        exchangeAddress = address(exchange);
        wallet1 = makeAddr("wallet1");
        wallet2 = makeAddr("wallet2");
    }

    function test_Deposit() public {
        (address usdcAddress, address btcAddress) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);

    }

    function test_MultipleDeposits() public {
        (address usdcAddress, address btcAddress) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet1, usdcAddress, 300e6);
        verifyBalances(wallet1, usdcAddress, 1300e6, 3700e6, 1300e6);

        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);
        deposit(wallet1, btcAddress, 33e8);
        verifyBalances(wallet1, btcAddress, 88e8, 12e8, 88e8);

    }

    function test_Withdrawal() public {
        (address usdcAddress, address btcAddress) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        withdraw(wallet1, usdcAddress, 133e6, 133e6);
        verifyBalances(wallet1, usdcAddress, 867e6, 4133e6, 867e6);

        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);
        withdraw(wallet1, btcAddress, 4e8, 4e8);
        verifyBalances(wallet1, btcAddress, 51e8, 49e8, 51e8);

        withdraw(wallet1, btcAddress, 0, 51e8);
        verifyBalances(wallet1, btcAddress, 0, 100e8, 0);

    }

    function test_MultipleWallets() public {
        (address usdcAddress, ) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet2, usdcAddress, 800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 4200e6, 1800e6);

        withdraw(wallet1, usdcAddress, 133e6, 133e6);
        verifyBalances(wallet1, usdcAddress, 867e6, 4133e6, 1667e6);
        withdraw(wallet2, usdcAddress, 120e6, 120e6);
        verifyBalances(wallet2, usdcAddress, 680e6, 4320e6, 1547e6);

    }

    function deposit(address wallet, address tokenAddress, uint256 amount) internal {
        vm.startPrank(wallet);
        IERC20(tokenAddress).approve(exchangeAddress, amount);
        exchange.deposit(tokenAddress, amount);
        vm.stopPrank();
    }

    function withdraw(address wallet, address tokenAddress, uint256 amount, uint256 expectedEmitAmount) internal {
        vm.startPrank(wallet);
        vm.expectEmit(exchangeAddress);
        emit Exchange.WithdrawalCreated(expectedEmitAmount);
        exchange.withdraw(tokenAddress, amount);
        vm.stopPrank();
    }

    function verifyBalances(address wallet, address tokenAddress, uint256 expectedBalance, uint256 walletBalance, uint256 exchangeBalance) internal {
        assertEq(exchange.balances(wallet, tokenAddress), expectedBalance);
        assertEq(IERC20(tokenAddress).balanceOf(wallet), walletBalance);
        assertEq(IERC20(tokenAddress).balanceOf(exchangeAddress), exchangeBalance);
    }

    function setupWallets() internal returns(address, address) {
        ERC20Mock usdcMock = new ERC20Mock();
        usdcMock.mint(wallet1, 5000e6);
        usdcMock.mint(wallet2, 5000e6);
        ERC20Mock btcMock = new ERC20Mock();
        btcMock.mint(wallet1, 100e8);
        btcMock.mint(wallet2, 100e8);
        return (address(usdcMock), address(btcMock));
    }
}
