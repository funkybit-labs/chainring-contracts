// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import {Test, console} from "forge-std/Test.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {ICoinProxy} from "../src/interfaces/ICoinProxy.sol";
import {CoinProxy} from "../src/CoinProxy.sol";
import {ERC1967Proxy} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract CoinProxyBaseTest is Test {
    CoinProxy internal coinProxy;
    address payable internal coinProxyProxyAddress;

    error OwnableUnauthorizedAccount(address account);

    uint256 internal submitterPrivateKey = 0x1234;
    address internal submitter = vm.addr(submitterPrivateKey);

    uint256 internal feeAccountPrivateKey = 0x12345;
    address internal feeAccount = vm.addr(feeAccountPrivateKey);

    uint256 internal wallet1PrivateKey = 0x12345678;
    uint256 internal wallet2PrivateKey = 0x123456789;
    address internal wallet1 = vm.addr(wallet1PrivateKey);
    address internal wallet2 = vm.addr(wallet2PrivateKey);

    address internal coinAddress;
    address internal btcAddress;

    function setUp() public virtual {
        CoinProxy coinProxyImplementation = new CoinProxy();
        coinProxyProxyAddress = payable(address(new ERC1967Proxy(address(coinProxyImplementation), "")));
        coinProxy = CoinProxy(coinProxyProxyAddress);
        coinProxy.initialize(submitter, feeAccount);
        assertEq(coinProxy.getVersion(), 1);
        vm.deal(submitter, 10 ether);
    }

    function deposit(address wallet, address tokenAddress, uint256 amount, uint64 sequence) internal {
        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.DepositSucceeded(wallet, sequence, tokenAddress, amount);
        ICoinProxy.BatchDeposit memory batch = ICoinProxy.BatchDeposit(new ICoinProxy.Deposit[](1));
        batch.deposits[0] =
            ICoinProxy.Deposit({sequence: sequence, sender: wallet, token: tokenAddress, amount: amount});
        coinProxy.submitDeposits(abi.encode(batch));
        vm.stopPrank();
    }

    function withdraw(
        address wallet,
        address tokenAddress,
        uint256 amount,
        uint256 expectedAmount,
        uint256 feeAmount,
        uint64 sequence
    ) internal {
        ICoinProxy.BatchWithdrawal memory batch = ICoinProxy.BatchWithdrawal(new ICoinProxy.Withdrawal[](1));
        batch.withdrawals[0] = ICoinProxy.Withdrawal({
            sequence: sequence,
            sender: wallet,
            token: tokenAddress,
            amount: amount,
            feeAmount: feeAmount
        });

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        if (amount != expectedAmount) {
            emit ICoinProxy.WithdrawalFailed(
                wallet, sequence, tokenAddress, amount, expectedAmount, ICoinProxy.ErrorCode.InsufficientBalance
            );
        } else {
            emit ICoinProxy.WithdrawalSucceeded(wallet, sequence, tokenAddress, expectedAmount, feeAmount);
        }
        coinProxy.submitWithdrawalBatch(abi.encode(batch));
        vm.stopPrank();
    }

    function withdrawInsufficientFee(
        address wallet,
        address tokenAddress,
        uint256 amount,
        uint256 expectedFeeBalance,
        uint256 feeAmount,
        uint64 sequence
    ) internal {
        ICoinProxy.BatchWithdrawal memory batch = ICoinProxy.BatchWithdrawal(new ICoinProxy.Withdrawal[](1));
        batch.withdrawals[0] = ICoinProxy.Withdrawal({
            sequence: sequence,
            sender: wallet,
            token: tokenAddress,
            amount: amount,
            feeAmount: feeAmount
        });

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.WithdrawalFailed(
            wallet, sequence, tokenAddress, feeAmount, expectedFeeBalance, ICoinProxy.ErrorCode.InsufficientFeeBalance
        );
        coinProxy.submitWithdrawalBatch(abi.encode(batch));
        vm.stopPrank();
    }

    function verifyBalance(address wallet, address tokenAddress, uint256 expectedBalance) internal view {
        assertEq(coinProxy.balances(wallet, tokenAddress), expectedBalance);
    }

    function verifyBalance(address wallet, uint256 expectedBalance, uint256 walletBalance) internal view {
        assertEq(coinProxy.balances(wallet, address(0)), expectedBalance);
        assertEq(wallet.balance, walletBalance);
    }

    function setupWallets() internal {
        coinAddress = vm.addr(0x123456789ABCDEF);
        btcAddress = address(0);
    }
}
