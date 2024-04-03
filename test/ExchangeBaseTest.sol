// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import {Test, console} from "forge-std/Test.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {Exchange} from "../src/Exchange.sol";
import {ERC1967Proxy} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract ExchangeBaseTest is Test {
    Exchange internal exchange;
    address payable internal exchangeProxyAddress;

    error OwnableUnauthorizedAccount(address account);

    uint256 internal submitterPrivateKey = 0x1234;
    address internal submitter = vm.addr(submitterPrivateKey);

    uint256 internal feeAccountPrivateKey = 0x12345;
    address internal feeAccount = vm.addr(feeAccountPrivateKey);

    function setUp() public virtual {
        Exchange exchangeImplementation = new Exchange();
        exchangeProxyAddress = payable(address(new ERC1967Proxy(address(exchangeImplementation), "")));
        exchange = Exchange(exchangeProxyAddress);
        exchange.initialize(submitter, feeAccount, 18);
        assertEq(exchange.getVersion(), 1);
        vm.deal(submitter, 10 ether);
    }

    function deposit(address wallet, address tokenAddress, uint256 amount) internal {
        vm.startPrank(wallet);
        IERC20(tokenAddress).approve(exchangeProxyAddress, amount);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Deposit(address(wallet), tokenAddress, amount);
        exchange.deposit(tokenAddress, amount);
        vm.stopPrank();
    }

    function deposit(address wallet, uint256 amount) internal {
        vm.startPrank(wallet);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Deposit(address(wallet), address(0), amount);
        (bool s,) = exchangeProxyAddress.call{value: amount}("");
        require(s);
        vm.stopPrank();
    }

    function withdraw(address wallet, address tokenAddress, uint256 amount, uint256 expectedEmitAmount) internal {
        vm.startPrank(wallet);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Withdrawal(wallet, tokenAddress, expectedEmitAmount);
        exchange.withdraw(tokenAddress, amount);
        vm.stopPrank();
    }

    function packTx(IExchange.TransactionType txType, bytes memory data) internal pure returns (bytes memory) {
        return abi.encodePacked(
            uint8(txType),
            uint256(0x20), // offset where data starts
            data
        );
    }

    function withdraw(address wallet, uint256 amount, uint256 expectedEmitAmount) internal {
        vm.startPrank(wallet);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Withdrawal(wallet, address(0), expectedEmitAmount);
        exchange.withdraw(amount);
        vm.stopPrank();
    }

    function verifyBalances(
        address wallet,
        address tokenAddress,
        uint256 expectedBalance,
        uint256 walletBalance,
        uint256 exchangeBalance
    ) internal view {
        assertEq(exchange.balances(wallet, tokenAddress), expectedBalance);
        assertEq(IERC20(tokenAddress).balanceOf(wallet), walletBalance);
        assertEq(IERC20(tokenAddress).balanceOf(exchangeProxyAddress), exchangeBalance);
    }

    function verifyBalances(address wallet, uint256 expectedBalance, uint256 walletBalance, uint256 exchangeBalance)
        internal
        view
    {
        assertEq(exchange.nativeBalances(wallet), expectedBalance);
        assertEq(wallet.balance, walletBalance);
        assertEq(exchangeProxyAddress.balance, exchangeBalance);
    }

    function sign(uint256 privateKey, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, digest);
        return abi.encodePacked(r, s, v);
    }
}
