// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {Exchange} from "../src/Exchange.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./utils/SigUtils.sol";
import {ExchangeBaseTest} from "./ExchangeBaseTest.sol";
import "../src/interfaces/IExchange.sol";
import "../src/interfaces/IExchange.sol";
import "../src/interfaces/IExchange.sol";

contract BatchSettlementTest is ExchangeBaseTest {
    uint256 internal takerPrivateKey = wallet1PrivateKey;
    uint256 internal makerPrivateKey = wallet2PrivateKey;
    address internal taker = vm.addr(takerPrivateKey);
    address internal maker = vm.addr(makerPrivateKey);

    function setUp() public override {
        super.setUp();
        vm.deal(taker, 10 ether);
        vm.deal(maker, 10 ether);
    }

    function test_SettleBuyTrade_BTC_USDC() public {
        setupWallets();

        deposit(taker, usdcAddress, 200000e6);
        verifyBalances(taker, usdcAddress, 200000e6, 300000e6, 200000e6);
        verifyBalances(taker, btcAddress, 0, 100e8, 0);
        deposit(maker, btcAddress, 55e8);
        verifyBalances(maker, usdcAddress, 0, 500000e6, 200000e6);
        verifyBalances(maker, btcAddress, 55e8, 45e8, 55e8);

        // taker will buy 2 BTC
        // taker fee will be 100 USDC and maker fee will be 20 USDC

        bytes32[] memory takerHashes = new bytes32[](1);
        takerHashes[0] = keccak256(bytes("order1:order2"));
        bytes32[] memory makerHashes = new bytes32[](1);
        makerHashes[0] = keccak256(bytes("order3:order4"));
        IExchange.BatchSettlement memory batch = IExchange.BatchSettlement(
            new address[](2), new IExchange.WalletTradeList[](2), new IExchange.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = IExchange.WalletTradeList(takerHashes);
        batch.walletTradeLists[1] = IExchange.WalletTradeList(makerHashes);
        batch.tokenAdjustmentLists[0] =
            IExchange.TokenAdjustmentList(btcAddress, getAdjustment(0, 2e8), getAdjustment(1, 2e8), 0);
        batch.tokenAdjustmentLists[1] = IExchange.TokenAdjustmentList(
            usdcAddress, getAdjustment(1, 140000e6 - 20e6), getAdjustment(0, 140000e6 + 100e6), 120e6
        );

        vm.startPrank(submitter);
        exchange.prepareSettlementBatch(abi.encode(batch));
        assertEq(exchange.batchHash(), keccak256(abi.encode(batch)));
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementCompleted(taker, takerHashes);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementCompleted(maker, makerHashes);
        exchange.submitSettlementBatch(abi.encode(batch));
        assertEq(exchange.lastSettlementBatchHash(), keccak256(abi.encode(batch)));
        vm.stopPrank();

        verifyBalances(taker, btcAddress, 2e8, 100e8, 55e8);
        verifyBalances(maker, btcAddress, 55e8 - 2e8, 45e8, 55e8);
        verifyBalances(taker, usdcAddress, 200000e6 - 140000e6 - 100e6, 300000e6, 200000e6);
        verifyBalances(maker, usdcAddress, 140000e6 - 20e6, 500000e6, 200000e6);
        verifyBalances(feeAccount, usdcAddress, 100e6 + 20e6, 0, 200000e6);
    }

    function test_SettleSellTrade_BTC_USDC() public {
        setupWallets();

        deposit(taker, btcAddress, 5e8);
        verifyBalances(taker, usdcAddress, 0, 500000e6, 0);
        verifyBalances(taker, btcAddress, 5e8, 95e8, 5e8);
        deposit(maker, usdcAddress, 200000e6);
        verifyBalances(maker, usdcAddress, 200000e6, 300000e6, 200000e6);
        verifyBalances(maker, btcAddress, 0, 100e8, 5e8);

        // taker will sell 2 BTC, for a price of 70000 USDC per BTC
        // taker fee will be 100 USDC and maker fee will be 20 USDC
        bytes32[] memory hashes = new bytes32[](1);
        hashes[0] = keccak256(bytes("order1:order2"));
        IExchange.BatchSettlement memory batch = IExchange.BatchSettlement(
            new address[](2), new IExchange.WalletTradeList[](2), new IExchange.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = IExchange.WalletTradeList(hashes);
        batch.walletTradeLists[1] = IExchange.WalletTradeList(hashes);
        batch.tokenAdjustmentLists[0] =
            IExchange.TokenAdjustmentList(btcAddress, getAdjustment(1, 2e8), getAdjustment(0, 2e8), 0);
        batch.tokenAdjustmentLists[1] = IExchange.TokenAdjustmentList(
            usdcAddress, getAdjustment(0, 140000e6 - 100e6), getAdjustment(1, 140000e6 + 20e6), 120e6
        );

        vm.startPrank(submitter);
        exchange.prepareSettlementBatch(abi.encode(batch));
        assertEq(exchange.batchHash(), keccak256(abi.encode(batch)));
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementCompleted(taker, hashes);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementCompleted(maker, hashes);
        exchange.submitSettlementBatch(abi.encode(batch));
        assertEq(exchange.lastSettlementBatchHash(), keccak256(abi.encode(batch)));
        vm.stopPrank();

        verifyBalances(taker, btcAddress, 3e8, 95e8, 5e8);
        verifyBalances(maker, btcAddress, 2e8, 100e8, 5e8);
        verifyBalances(taker, usdcAddress, 140000e6 - 100e6, 500000e6, 200000e6);
        verifyBalances(maker, usdcAddress, 200000e6 - 140000e6 - 20e6, 300000e6, 200000e6);
        verifyBalances(feeAccount, usdcAddress, 100e6 + 20e6, 0, 200000e6);
    }

    function test_SettleNativeTrade_BTC_ETH() public {
        setupWallets();

        deposit(taker, 3e18);
        verifyBalances(taker, btcAddress, 0, 100e8, 0);
        verifyBalances(taker, 3e18, 7e18, 3e18);
        deposit(maker, btcAddress, 2e8);
        verifyBalances(maker, btcAddress, 2e8, 98e8, 2e8);
        verifyBalances(maker, 0, 10e18, 3e18);

        // taker will buy .1 BTC, price is 20 ETH per BTC so will need to pay 2ETH, takerFee will be .02 ETH and makerFee will be 0.01 ETH
        bytes32[] memory hashes = new bytes32[](1);
        hashes[0] = keccak256(bytes("order1:order2"));
        IExchange.BatchSettlement memory batch = IExchange.BatchSettlement(
            new address[](2), new IExchange.WalletTradeList[](2), new IExchange.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = IExchange.WalletTradeList(hashes);
        batch.walletTradeLists[1] = IExchange.WalletTradeList(hashes);
        batch.tokenAdjustmentLists[0] =
            IExchange.TokenAdjustmentList(btcAddress, getAdjustment(0, 1e7), getAdjustment(1, 1e7), 0);
        batch.tokenAdjustmentLists[1] = IExchange.TokenAdjustmentList(
            address(0), getAdjustment(1, 2e18 - 1e16), getAdjustment(0, 2e18 + 2e16), 3e16
        );

        vm.startPrank(submitter);
        exchange.prepareSettlementBatch(abi.encode(batch));
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementCompleted(taker, hashes);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementCompleted(maker, hashes);
        exchange.submitSettlementBatch(abi.encode(batch));
        vm.stopPrank();

        verifyBalances(taker, btcAddress, 1e7, 100e8, 2e8);
        verifyBalances(taker, 3e18 - 2e18 - 2e16, 7e18, 3e18);
        verifyBalances(maker, btcAddress, 2e8 - 1e7, 98e8, 2e8);
        verifyBalances(maker, 2e18 - 1e16, 10e18, 3e18);
        verifyBalances(feeAccount, 3e16, 0, 3e18);
    }

    function test_Settlement_Does_Not_Net_To_Zero() public {
        setupWallets();

        deposit(taker, 3e18);
        verifyBalances(taker, btcAddress, 0, 100e8, 0);
        verifyBalances(taker, 3e18, 7e18, 3e18);
        deposit(maker, btcAddress, 2e8);
        verifyBalances(maker, btcAddress, 2e8, 98e8, 2e8);
        verifyBalances(maker, 0, 10e18, 3e18);

        // taker will buy .1 BTC, price is 20 ETH per BTC so will need to pay 2ETH, takerFee will be .02 ETH and makerFee will be 0.01 ETH
        bytes32[] memory hashes = new bytes32[](1);
        hashes[0] = keccak256(bytes("order1:order2"));
        IExchange.BatchSettlement memory batch = IExchange.BatchSettlement(
            new address[](2), new IExchange.WalletTradeList[](2), new IExchange.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = IExchange.WalletTradeList(hashes);
        batch.walletTradeLists[1] = IExchange.WalletTradeList(hashes);
        batch.tokenAdjustmentLists[0] =
            IExchange.TokenAdjustmentList(btcAddress, getAdjustment(0, 1e7), getAdjustment(1, 1e7), 0);
        batch.tokenAdjustmentLists[1] = IExchange.TokenAdjustmentList(
            address(0), getAdjustment(1, 2e18 - 1e16), getAdjustment(0, 2e18 + 2e16), 2e16
        );

        vm.startPrank(submitter);
        vm.expectRevert(abi.encodeWithSelector(IExchange.ErrorDidNotNetToZero.selector, address(0)));
        exchange.prepareSettlementBatch(abi.encode(batch));
        vm.stopPrank();
    }

    function test_Settlement_Failures() public {
        setupWallets();

        deposit(taker, 3e18);
        verifyBalances(taker, btcAddress, 0, 100e8, 0);
        verifyBalances(taker, 3e18, 7e18, 3e18);
        //deposit(maker, btcAddress, 2e8);
        verifyBalances(maker, btcAddress, 0, 100e8, 0);
        verifyBalances(maker, 0, 10e18, 3e18);

        // taker will buy .1 BTC, price is 20 ETH per BTC so will need to pay 2ETH, takerFee will be .02 ETH and makerFee will be 0.01 ETH
        bytes32[] memory takerHashes = new bytes32[](1);
        takerHashes[0] = keccak256(bytes("order1:order2"));
        bytes32[] memory makerHashes = new bytes32[](1);
        makerHashes[0] = keccak256(bytes("order3:order4"));
        IExchange.BatchSettlement memory batch = IExchange.BatchSettlement(
            new address[](2), new IExchange.WalletTradeList[](2), new IExchange.TokenAdjustmentList[](1)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = IExchange.WalletTradeList(takerHashes);
        batch.walletTradeLists[1] = IExchange.WalletTradeList(makerHashes);
        batch.tokenAdjustmentLists[0] =
            IExchange.TokenAdjustmentList(btcAddress, getAdjustment(0, 1e7), getAdjustment(1, 1e7), 0);

        vm.startPrank(submitter);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.SettlementFailed(maker, makerHashes, IExchange.ErrorCode.InsufficientBalance);
        exchange.prepareSettlementBatch(abi.encode(batch));

        // since prepare had failures, batch should be rolled back, make sure cannot submit
        vm.expectRevert(bytes("No batch prepared"));
        exchange.submitSettlementBatch(abi.encode(batch));
        vm.stopPrank();

        deposit(maker, btcAddress, 2e8);

        vm.startPrank(submitter);
        exchange.prepareSettlementBatch(abi.encode(batch));
        // change something before submitting - should not match
        batch.walletTradeLists[1] = IExchange.WalletTradeList(takerHashes);
        vm.expectRevert(bytes("Hash does not match prepared batch"));
        exchange.submitSettlementBatch(abi.encode(batch));
        exchange.rollbackBatch();
        vm.stopPrank();

        // only the submitter can call prepare / submit
        vm.startPrank(taker);
        vm.expectRevert(bytes("Sender is not the submitter"));
        exchange.prepareSettlementBatch(abi.encode(batch));
        vm.expectRevert(bytes("Sender is not the submitter"));
        exchange.submitSettlementBatch(abi.encode(batch));
    }

    function getAdjustment(uint16 walletIndex, uint256 amount) internal pure returns (IExchange.Adjustment[] memory) {
        IExchange.Adjustment[] memory adjustments = new IExchange.Adjustment[](1);
        adjustments[0] = IExchange.Adjustment(walletIndex, amount);
        return adjustments;
    }
}
