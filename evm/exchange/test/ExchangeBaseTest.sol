// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import {Test, console} from "forge-std/Test.sol";
import {MockERC20} from "./contracts/MockERC20.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {Exchange} from "../src/Exchange.sol";
import {ERC1967Proxy} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import "./utils/SigUtils.sol";

contract ExchangeBaseTest is Test {
    Exchange internal exchange;
    address payable internal exchangeProxyAddress;

    error OwnableUnauthorizedAccount(address account);

    uint256 internal submitterPrivateKey = 0x1234;
    address internal submitter = vm.addr(submitterPrivateKey);

    uint256 internal feeAccountPrivateKey = 0x12345;
    address internal feeAccount = vm.addr(feeAccountPrivateKey);

    uint256 internal wallet1PrivateKey = 0x12345678;
    uint256 internal wallet2PrivateKey = 0x123456789;
    address internal wallet1 = vm.addr(wallet1PrivateKey);
    address internal wallet2 = vm.addr(wallet2PrivateKey);

    address internal usdcAddress;
    address internal btcAddress;

    function setUp() public virtual {
        Exchange exchangeImplementation = new Exchange();
        exchangeProxyAddress = payable(address(new ERC1967Proxy(address(exchangeImplementation), "")));
        exchange = Exchange(exchangeProxyAddress);
        exchange.initialize(submitter, feeAccount, 1 minutes);
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

    function withdrawAll(
        uint256 walletPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint256 expectedAmount,
        uint256 feeAmount
    ) internal {
        withdraw(walletPrivateKey, tokenAddress, amount, expectedAmount, feeAmount, true);
    }

    function withdraw(
        uint256 walletPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint256 expectedAmount,
        uint256 feeAmount
    ) internal {
        withdraw(walletPrivateKey, tokenAddress, amount, expectedAmount, feeAmount, false);
    }

    function withdraw(
        address wallet,
        uint256 linkedSignerPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint256 expectedAmount,
        uint256 feeAmount,
        bool expectSignatureFailure
    ) internal {
        uint64 sequence = 1;
        bytes memory tx1 =
            createSignedWithdrawTx(linkedSignerPrivateKey, tokenAddress, amount, 1000, sequence, feeAmount, wallet);
        bytes[] memory txs = new bytes[](1);
        txs[0] = tx1;

        vm.startPrank(submitter);
        vm.expectEmit(exchangeProxyAddress);
        if (expectSignatureFailure || amount != expectedAmount) {
            emit IExchange.WithdrawalFailed(
                wallet,
                sequence,
                tokenAddress,
                amount,
                expectedAmount,
                expectSignatureFailure ? IExchange.ErrorCode.InvalidSignature : IExchange.ErrorCode.InsufficientBalance
            );
        } else {
            emit IExchange.Withdrawal(wallet, sequence, tokenAddress, expectedAmount, feeAmount);
        }
        exchange.submitWithdrawals(txs);
        vm.stopPrank();
    }

    function withdraw(
        uint256 walletPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint256 expectedAmount,
        uint256 feeAmount,
        bool isWithdrawAll
    ) internal {
        uint64 sequence = 1;
        bytes memory tx1 = isWithdrawAll
            ? createSignedWithdrawAllTx(walletPrivateKey, tokenAddress, amount, 1000, sequence, feeAmount)
            : createSignedWithdrawTx(walletPrivateKey, tokenAddress, amount, 1000, sequence, feeAmount, address(0));
        bytes[] memory txs = new bytes[](1);
        txs[0] = tx1;

        vm.startPrank(submitter);
        vm.expectEmit(exchangeProxyAddress);
        if (!isWithdrawAll && amount != expectedAmount) {
            emit IExchange.WithdrawalFailed(
                vm.addr(walletPrivateKey),
                sequence,
                tokenAddress,
                amount,
                expectedAmount,
                IExchange.ErrorCode.InsufficientBalance
            );
        } else {
            emit IExchange.Withdrawal(vm.addr(walletPrivateKey), sequence, tokenAddress, expectedAmount, feeAmount);
        }
        exchange.submitWithdrawals(txs);
        vm.stopPrank();
    }

    function linkSigner(address wallet, uint256 linkedSignerPrivateKey) internal {
        vm.startPrank(wallet);
        vm.expectEmit(exchangeProxyAddress);
        address signerAddress = vm.addr(linkedSignerPrivateKey);
        emit IExchange.LinkedSigner(wallet, signerAddress);
        bytes32 digest = keccak256(abi.encodePacked(linkedSignerPrivateKey));
        exchange.linkSigner(signerAddress, digest, sign(linkedSignerPrivateKey, digest));
        assertEq(exchange.linkedSigners(wallet), signerAddress);
        vm.stopPrank();
    }

    function removeLinkedSigner(address wallet) internal {
        vm.startPrank(wallet);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.LinkedSigner(wallet, address(0));
        exchange.removeLinkedSigner();
        assertEq(exchange.linkedSigners(wallet), address(0));
        vm.stopPrank();
    }

    function packTx(IExchange.TransactionType txType, bytes memory data) internal pure returns (bytes memory) {
        return abi.encodePacked(uint8(txType), data);
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
        assertEq(exchange.balances(wallet, address(0)), expectedBalance);
        assertEq(wallet.balance, walletBalance);
        assertEq(exchangeProxyAddress.balance, exchangeBalance);
    }

    function sign(uint256 privateKey, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, digest);
        return abi.encodePacked(r, s, v);
    }

    function createSignedWithdrawAllTx(
        uint256 walletPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint64 nonce,
        uint256 sequence,
        uint256 feeAmount
    ) internal view returns (bytes memory) {
        IExchange.Withdraw memory _withdraw = IExchange.Withdraw({
            sender: vm.addr(walletPrivateKey),
            token: tokenAddress,
            amount: 0,
            nonce: nonce,
            feeAmount: feeAmount
        });

        bytes32 digest = SigUtils.getTypedDataHash(exchange.DOMAIN_SEPARATOR(), SigUtils.getStructHash(_withdraw));
        _withdraw.amount = amount;

        bytes memory signature = sign(walletPrivateKey, digest);

        IExchange.WithdrawWithSignature memory _withdrawWithSignature =
            IExchange.WithdrawWithSignature(uint64(sequence), _withdraw, signature);
        return packTx(IExchange.TransactionType.WithdrawAll, abi.encode(_withdrawWithSignature));
    }

    function createSignedWithdrawTx(
        uint256 signerPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint64 nonce,
        uint256 sequence,
        uint256 feeAmount,
        address sender
    ) internal view returns (bytes memory) {
        IExchange.Withdraw memory _withdraw = IExchange.Withdraw({
            sender: sender != address(0) ? sender : vm.addr(signerPrivateKey),
            token: tokenAddress,
            amount: amount,
            nonce: nonce,
            feeAmount: feeAmount
        });

        bytes32 digest = SigUtils.getTypedDataHash(exchange.DOMAIN_SEPARATOR(), SigUtils.getStructHash(_withdraw));

        bytes memory signature = sign(signerPrivateKey, digest);

        IExchange.WithdrawWithSignature memory _withdrawWithSignature =
            IExchange.WithdrawWithSignature(uint64(sequence), _withdraw, signature);
        return packTx(IExchange.TransactionType.Withdraw, abi.encode(_withdrawWithSignature));
    }

    function createSignedWithdrawTxWithInvalidSignature(
        uint256 walletPrivateKey,
        address tokenAddress,
        uint256 amount,
        uint64 nonce,
        uint256 sequence
    ) internal view returns (bytes memory) {
        IExchange.Withdraw memory _withdraw =
            IExchange.Withdraw({sender: address(0), token: tokenAddress, amount: amount, nonce: nonce, feeAmount: 0});
        IExchange.Withdraw memory _withdraw2 =
            IExchange.Withdraw({sender: address(1), token: tokenAddress, amount: amount, nonce: nonce, feeAmount: 0});

        bytes32 digest = SigUtils.getTypedDataHash(exchange.DOMAIN_SEPARATOR(), SigUtils.getStructHash(_withdraw));

        bytes memory signature = sign(walletPrivateKey, digest);

        IExchange.WithdrawWithSignature memory _withdrawWithSignature =
            IExchange.WithdrawWithSignature(uint64(sequence), _withdraw2, signature);
        return packTx(IExchange.TransactionType.Withdraw, abi.encode(_withdrawWithSignature));
    }

    function setupWallets() internal {
        MockERC20 usdcMock = new MockERC20("USD Coin", "USDC", 6);
        usdcMock.mint(wallet1, 500000e6);
        usdcMock.mint(wallet2, 500000e6);
        MockERC20 btcMock = new MockERC20("Bitcoin", "BTC", 8);
        btcMock.mint(wallet1, 100e8);
        btcMock.mint(wallet2, 100e8);

        usdcAddress = address(usdcMock);
        btcAddress = address(btcMock);
    }
}
