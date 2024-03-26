// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {MockERC20} from "./contracts/MockERC20.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {Exchange} from "../src/Exchange.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {ERC1967Proxy} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./utils/SigUtils.sol";
import "./contracts/ExchangeUpgrade.sol";

contract ExchangeTest is Test {
    Exchange internal exchange;
    address payable internal exchangeProxyAddress;
    uint256 internal wallet1PrivateKey = 0x12345678;
    uint256 internal wallet2PrivateKey = 0x123456789;
    address internal wallet1 = vm.addr(wallet1PrivateKey);
    address internal wallet2 = vm.addr(wallet2PrivateKey);

    error OwnableUnauthorizedAccount(address account);

    uint256 internal submitterPrivateKey = 0x1234;
    address internal submitter = vm.addr(submitterPrivateKey);

    function setUp() public {
        Exchange exchangeImplementation = new Exchange();
        exchangeProxyAddress = payable(address(new ERC1967Proxy(address(exchangeImplementation), "")));
        exchange = Exchange(exchangeProxyAddress);
        exchange.initialize(submitter);
        assertEq(exchange.getVersion(), 1);
        vm.deal(wallet1, 10 ether);
        vm.deal(wallet2, 10 ether);
        vm.deal(submitter, 10 ether);
    }

    function test_ERC20Deposit() public {
        (address usdcAddress, address btcAddress) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);
    }

    function test_MultipleERC20Deposits() public {
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

    function test_ERC20Withdrawal() public {
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
        (address usdcAddress,) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet2, usdcAddress, 800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 4200e6, 1800e6);

        withdraw(wallet1, usdcAddress, 133e6, 133e6);
        verifyBalances(wallet1, usdcAddress, 867e6, 4133e6, 1667e6);
        withdraw(wallet2, usdcAddress, 120e6, 120e6);
        verifyBalances(wallet2, usdcAddress, 680e6, 4320e6, 1547e6);
    }

    function test_Upgrade() public {
        (address usdcAddress,) = setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet2, usdcAddress, 800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 4200e6, 1800e6);

        // test that only the owner can upgrade the contract
        vm.startPrank(wallet1);
        ExchangeUpgrade newImplementation = new ExchangeUpgrade();
        vm.expectRevert(abi.encodeWithSelector(OwnableUnauthorizedAccount.selector, wallet1));
        exchange.upgradeToAndCall(address(newImplementation), "");
        vm.stopPrank();

        // call the proxy this time as the owner to perform the upgrade and also send data to invoke a function
        // in the new implementation as part of the upgrade
        // we should verify we get an Upgraded event with the implementation contract address from the proxy
        vm.startPrank(exchange.owner());
        vm.expectEmit(exchangeProxyAddress);
        emit ERC1967Utils.Upgraded(address(newImplementation));
        exchange.upgradeToAndCall(
            address(newImplementation), abi.encodeWithSelector(ExchangeUpgrade.setValue.selector, 1000)
        );
        vm.stopPrank();
        // verify the new value in the upgraded contract is set as part of the upgrade
        assertEq(ExchangeUpgrade(exchangeProxyAddress).value(), 1000);

        // check balances are maintained after the upgrade
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 4200e6, 1800e6);

        // perform some withdrawals
        withdraw(wallet1, usdcAddress, 100e6, 100e6);
        verifyBalances(wallet1, usdcAddress, 900e6, 4100e6, 1700e6);
        withdraw(wallet2, usdcAddress, 120e6, 120e6);
        verifyBalances(wallet2, usdcAddress, 680e6, 4320e6, 1580e6);
    }

    function test_NativeDepositsAndWithdrawals() public {
        deposit(wallet1, 2e18);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        deposit(wallet2, 3e18);
        verifyBalances(wallet2, 3e18, 7e18, 5e18);

        withdraw(wallet1, 1e18, 1e18);
        verifyBalances(wallet1, 1e18, 9e18, 4e18);

        withdraw(wallet2, 1e18, 1e18);
        verifyBalances(wallet2, 2e18, 8e18, 3e18);

        // test withdrawal all
        withdraw(wallet2, 0e18, 2e18);
        verifyBalances(wallet2, 0e18, 10e18, 1e18);
    }

    function test_EIP712Withdrawals() public {
        (address usdcAddress,) = setupWallets();

        // wallet1 - deposit usdc and native token
        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet1, 2e18);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        // wallet2 - deposit USDC
        deposit(wallet2, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 2000e6);

        uint64 wallet1Nonce = exchange.nonces(wallet1);
        bytes memory tx1 = createSignedWithdrawTx(wallet1PrivateKey, usdcAddress, 200e6, wallet1Nonce);
        bytes memory tx2 = createSignedWithdrawNativeTx(wallet1PrivateKey, 1e18, wallet1Nonce + 1);
        uint64 wallet2Nonce = exchange.nonces(wallet2);
        bytes memory tx3 = createSignedWithdrawTx(wallet2PrivateKey, usdcAddress, 300e6, wallet2Nonce);

        uint256 txProcessedCount = exchange.txProcessedCount();

        bytes[] memory txs = new bytes[](3);
        txs[0] = tx1;
        txs[1] = tx2;
        txs[2] = tx3;
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Withdrawal(wallet1, usdcAddress, 200e6);
        emit IExchange.Withdrawal(wallet1, address(0), 1e18);
        emit IExchange.Withdrawal(wallet2, usdcAddress, 300e6);
        vm.prank(submitter);
        exchange.submitTransactions(txs);

        // verify nonces
        assertEq(wallet1Nonce + 2, exchange.nonces(wallet1));
        assertEq(wallet2Nonce + 1, exchange.nonces(wallet2));

        // verify balances
        verifyBalances(wallet1, usdcAddress, 800e6, 4200e6, 1500e6);
        verifyBalances(wallet1, 1e18, 9e18, 1e18);
        verifyBalances(wallet2, usdcAddress, 700e6, 4300e6, 1500e6);

        assertEq(txProcessedCount + 3, exchange.txProcessedCount());
    }

    function test_ErrorCases() public {
        (address usdcAddress,) = setupWallets();
        deposit(wallet1, usdcAddress, 1000e6);
        vm.expectRevert(bytes("Insufficient Balance"));
        vm.startPrank(wallet1);
        exchange.withdraw(usdcAddress, 1001e6);
        vm.stopPrank();

        deposit(wallet1, 2e18);
        vm.expectRevert(bytes("Insufficient Balance"));
        vm.startPrank(wallet1);
        exchange.withdraw(3e18);
        vm.stopPrank();
    }

    function test_EIP712ErrorCases() public {
        (address usdcAddress,) = setupWallets();

        // wallet1 - deposit usdc and native token
        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        deposit(wallet1, 2e18);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        uint64 wallet1Nonce = exchange.nonces(wallet1);
        bytes memory tx1 = createSignedWithdrawTx(wallet1PrivateKey, usdcAddress, 200e6, wallet1Nonce);
        bytes memory tx2 = createSignedWithdrawNativeTx(wallet1PrivateKey, 1e18, wallet1Nonce + 2); // bad nonce
        bytes memory tx3 = createSignedWithdrawNativeTx(wallet1PrivateKey, 3e18, wallet1Nonce + 1); // insufficient balance
        uint256 txProcessedCount = exchange.txProcessedCount();

        bytes[] memory txs = new bytes[](2);
        txs[0] = tx1;
        txs[1] = tx2;

        // check fails if not the submitter
        vm.prank(wallet1);
        vm.expectRevert(bytes("Sender is not the submitter"));
        exchange.submitTransactions(txs);

        // bad nonce
        vm.prank(submitter);
        vm.expectRevert(bytes("Invalid Nonce"));
        exchange.submitTransactions(txs);

        // insufficient balance
        txs[1] = tx3;
        vm.prank(submitter);
        vm.expectRevert(bytes("Insufficient Balance"));
        exchange.submitTransactions(txs);

        // verify nothing has changed
        assertEq(wallet1Nonce, exchange.nonces(wallet1));
        assertEq(txProcessedCount, exchange.txProcessedCount());
        verifyBalances(wallet1, usdcAddress, 1000e6, 4000e6, 1000e6);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        // must be a valid address
        vm.expectRevert(bytes("Not a valid address"));
        exchange.setSubmitter(address(0));

        // only owner can change the submitter and
        vm.prank(wallet1);
        vm.expectRevert(abi.encodeWithSelector(OwnableUnauthorizedAccount.selector, wallet1));
        exchange.setSubmitter(submitter);

        // change the submitter and verify
        uint256 newSubmitterPrivateKey = 0x123456;
        address newSubmitter = vm.addr(newSubmitterPrivateKey);

        // change the submitter
        exchange.setSubmitter(newSubmitter);

        // should fail with old submitter
        vm.prank(submitter);
        vm.expectRevert(bytes("Sender is not the submitter"));
        exchange.submitTransactions(txs);

        // should fail differently with new submitter
        vm.prank(newSubmitter);
        vm.expectRevert(bytes("Insufficient Balance"));
        exchange.submitTransactions(txs);

        // set it back
        exchange.setSubmitter(submitter);
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

    function createSignedWithdrawTx(uint256 walletPrivateKey, address tokenAddress, uint256 amount, uint64 nonce)
        internal
        view
        returns (bytes memory)
    {
        IExchange.Withdraw memory _withdraw =
            IExchange.Withdraw({sender: vm.addr(walletPrivateKey), token: tokenAddress, amount: amount, nonce: nonce});

        bytes32 digest = SigUtils.getTypedDataHash(exchange.DOMAIN_SEPARATOR(), SigUtils.getStructHash(_withdraw));

        bytes memory signature = sign(walletPrivateKey, digest);
        return packTx(
            IExchange.TransactionType.Withdraw,
            abi.encode(_withdraw.sender, _withdraw.token, _withdraw.amount, _withdraw.nonce, signature)
        );
    }

    function createSignedWithdrawNativeTx(uint256 walletPrivateKey, uint256 amount, uint64 nonce)
        internal
        view
        returns (bytes memory)
    {
        IExchange.WithdrawNative memory _withdraw =
            IExchange.WithdrawNative({sender: vm.addr(walletPrivateKey), amount: amount, nonce: nonce});

        bytes32 digest = SigUtils.getTypedDataHash(exchange.DOMAIN_SEPARATOR(), SigUtils.getStructHash(_withdraw));

        bytes memory signature = sign(walletPrivateKey, digest);
        return packTx(
            IExchange.TransactionType.WithdrawNative,
            abi.encode(_withdraw.sender, _withdraw.amount, _withdraw.nonce, signature)
        );
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

    function setupWallets() internal returns (address, address) {
        MockERC20 usdcMock = new MockERC20("USD Coin", "USDC");
        usdcMock.mint(wallet1, 5000e6);
        usdcMock.mint(wallet2, 5000e6);
        MockERC20 btcMock = new MockERC20("Bitcoin", "BTC");
        btcMock.mint(wallet1, 100e8);
        btcMock.mint(wallet2, 100e8);
        return (address(usdcMock), address(btcMock));
    }

    function sign(uint256 privateKey, bytes32 digest) internal pure returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(privateKey, digest);
        return abi.encodePacked(r, s, v);
    }
}
