// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {CoinProxy} from "../src/CoinProxy.sol";
import {ICoinProxy} from "../src/interfaces/ICoinProxy.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import {CoinProxyBaseTest} from "./CoinProxyBaseTest.sol";
import "../src/interfaces/ICoinProxy.sol";
import "../src/interfaces/ICoinProxy.sol";
import "../src/interfaces/ICoinProxy.sol";

contract CoinSettlementTest is CoinProxyBaseTest {
    uint256 internal takerPrivateKey = wallet1PrivateKey;
    uint256 internal makerPrivateKey = wallet2PrivateKey;
    address internal taker = vm.addr(takerPrivateKey);
    address internal maker = vm.addr(makerPrivateKey);

    function setUp() public override {
        super.setUp();
        vm.deal(taker, 10 ether);
        vm.deal(maker, 10 ether);
    }

    function test_SettleTrade_BTC_COIN() public {
        setupWallets();

        deposit(maker, coinAddress, 200000e6, 1);
        verifyBalance(maker, coinAddress, 200000e6);
        verifyBalance(maker, btcAddress, 0);
        deposit(taker, btcAddress, 3e8, 2);
        verifyBalance(taker, btcAddress, 3e8);
        verifyBalance(taker, coinAddress, 0);

        // taker will buy 1000 coins for 2 BTC
        // taker fee will be .02 BTC and maker fee will be .01 BTC

        bytes32[] memory takerHashes = new bytes32[](1);
        takerHashes[0] = keccak256(bytes("order1:order2"));
        bytes32[] memory makerHashes = new bytes32[](1);
        makerHashes[0] = keccak256(bytes("order3:order4"));
        ICoinProxy.BatchSettlement memory batch = ICoinProxy.BatchSettlement(
            new address[](2), new ICoinProxy.WalletTradeList[](2), new ICoinProxy.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = ICoinProxy.WalletTradeList(takerHashes);
        batch.walletTradeLists[1] = ICoinProxy.WalletTradeList(makerHashes);
        batch.tokenAdjustmentLists[0] =
            ICoinProxy.TokenAdjustmentList(btcAddress, getAdjustment(1, 2e8 - 1e6), getAdjustment(0, 2e8 + 2e6), 3e6);
        batch.tokenAdjustmentLists[1] =
            ICoinProxy.TokenAdjustmentList(coinAddress, getAdjustment(0, 1000e6), getAdjustment(1, 1000e6), 0);

        vm.startPrank(submitter);
        coinProxy.prepareSettlementBatch(abi.encode(batch));
        assertEq(coinProxy.batchHash(), keccak256(abi.encode(batch)));
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.SettlementCompleted(taker, takerHashes);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.SettlementCompleted(maker, makerHashes);
        coinProxy.submitSettlementBatch(abi.encode(batch));
        assertEq(coinProxy.lastSettlementBatchHash(), keccak256(abi.encode(batch)));
        vm.stopPrank();

        verifyBalance(taker, btcAddress, 3e8 - 2e8 - 2e6);
        verifyBalance(maker, btcAddress, 2e8 - 1e6);
        verifyBalance(taker, coinAddress, 1000e6);
        verifyBalance(maker, coinAddress, 200000e6 - 1000e6);
        verifyBalance(feeAccount, btcAddress, 3e6);
    }

    function test_Settlement_Does_Not_Net_To_Zero() public {
        setupWallets();

        deposit(maker, coinAddress, 200000e6, 1);
        verifyBalance(maker, coinAddress, 200000e6);
        verifyBalance(maker, btcAddress, 0);
        deposit(taker, btcAddress, 3e8, 2);
        verifyBalance(taker, btcAddress, 3e8);
        verifyBalance(taker, coinAddress, 0);

        // taker will buy 1000 coins for 2 BTC
        // taker fee will be .02 BTC and maker fee will be .01 BTC

        bytes32[] memory takerHashes = new bytes32[](1);
        takerHashes[0] = keccak256(bytes("order1:order2"));
        bytes32[] memory makerHashes = new bytes32[](1);
        makerHashes[0] = keccak256(bytes("order3:order4"));
        ICoinProxy.BatchSettlement memory batch = ICoinProxy.BatchSettlement(
            new address[](2), new ICoinProxy.WalletTradeList[](2), new ICoinProxy.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = ICoinProxy.WalletTradeList(takerHashes);
        batch.walletTradeLists[1] = ICoinProxy.WalletTradeList(makerHashes);
        batch.tokenAdjustmentLists[0] =
            ICoinProxy.TokenAdjustmentList(btcAddress, getAdjustment(1, 2e8 - 1e6), getAdjustment(0, 2e8 + 2e6), 0);
        batch.tokenAdjustmentLists[1] =
            ICoinProxy.TokenAdjustmentList(coinAddress, getAdjustment(0, 1000e6), getAdjustment(1, 1000e6), 0);

        vm.startPrank(submitter);
        vm.expectRevert(abi.encodeWithSelector(ICoinProxy.ErrorDidNotNetToZero.selector, address(0)));
        coinProxy.prepareSettlementBatch(abi.encode(batch));
        vm.stopPrank();
    }

    function test_Settlement_Failures() public {
        setupWallets();

        deposit(maker, coinAddress, 200000e6, 1);
        verifyBalance(maker, coinAddress, 200000e6);
        verifyBalance(maker, btcAddress, 0);
        //deposit(taker, btcAddress, 3e8, 2);
        verifyBalance(taker, btcAddress, 0);
        verifyBalance(taker, coinAddress, 0);

        bytes32[] memory takerHashes = new bytes32[](1);
        takerHashes[0] = keccak256(bytes("order1:order2"));
        bytes32[] memory makerHashes = new bytes32[](1);
        makerHashes[0] = keccak256(bytes("order3:order4"));
        ICoinProxy.BatchSettlement memory batch = ICoinProxy.BatchSettlement(
            new address[](2), new ICoinProxy.WalletTradeList[](2), new ICoinProxy.TokenAdjustmentList[](2)
        );
        batch.walletAddresses[0] = taker;
        batch.walletAddresses[1] = maker;
        batch.walletTradeLists[0] = ICoinProxy.WalletTradeList(takerHashes);
        batch.walletTradeLists[1] = ICoinProxy.WalletTradeList(makerHashes);
        batch.tokenAdjustmentLists[0] =
            ICoinProxy.TokenAdjustmentList(btcAddress, getAdjustment(1, 2e8 - 1e6), getAdjustment(0, 2e8 + 2e6), 3e6);
        batch.tokenAdjustmentLists[1] =
            ICoinProxy.TokenAdjustmentList(coinAddress, getAdjustment(0, 1000e6), getAdjustment(1, 1000e6), 0);

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.SettlementFailed(taker, btcAddress, takerHashes, 2e8 + 2e6, 0);
        coinProxy.prepareSettlementBatch(abi.encode(batch));

        // since prepare had failures, batch should be rolled back, make sure cannot submit
        vm.expectRevert(bytes("No batch prepared"));
        coinProxy.submitSettlementBatch(abi.encode(batch));
        vm.stopPrank();

        deposit(taker, btcAddress, 3e8, 2);

        vm.startPrank(submitter);
        coinProxy.prepareSettlementBatch(abi.encode(batch));
        // change something before submitting - should not match
        batch.walletTradeLists[1] = ICoinProxy.WalletTradeList(takerHashes);
        vm.expectRevert(bytes("Hash does not match prepared batch"));
        coinProxy.submitSettlementBatch(abi.encode(batch));
        coinProxy.rollbackBatch();
        vm.stopPrank();

        // only the submitter can call prepare / submit
        vm.startPrank(taker);
        vm.expectRevert(bytes("Sender is not the submitter"));
        coinProxy.prepareSettlementBatch(abi.encode(batch));
        vm.expectRevert(bytes("Sender is not the submitter"));
        coinProxy.submitSettlementBatch(abi.encode(batch));
    }

    function getAdjustment(uint16 walletIndex, uint256 amount) internal pure returns (ICoinProxy.Adjustment[] memory) {
        ICoinProxy.Adjustment[] memory adjustments = new ICoinProxy.Adjustment[](1);
        adjustments[0] = ICoinProxy.Adjustment(walletIndex, amount);
        return adjustments;
    }
}
