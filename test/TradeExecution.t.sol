// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {MockERC20} from "./contracts/MockERC20.sol";
import {Exchange} from "../src/Exchange.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./utils/SigUtils.sol";
import {ExchangeBaseTest} from "./ExchangeBaseTest.sol";

contract TradeExecutionTest is ExchangeBaseTest {
    uint256 internal takerPrivateKey = 0x12345678;
    uint256 internal makerPrivateKey = 0x123456789;
    address internal taker = vm.addr(takerPrivateKey);
    address internal maker = vm.addr(makerPrivateKey);
    address internal usdcAddress;
    address internal btcAddress;

    function setUp() public override {
        super.setUp();
        vm.deal(taker, 10 ether);
        vm.deal(maker, 10 ether);
    }

    function test_SingleBuyTrade_BTC_USDC() public {
        setupWallets();

        deposit(taker, usdcAddress, 200000e6);
        verifyBalances(taker, usdcAddress, 200000e6, 300000e6, 200000e6);
        verifyBalances(taker, btcAddress, 0, 100e8, 0);
        deposit(maker, btcAddress, 55e8);
        verifyBalances(maker, usdcAddress, 0, 500000e6, 200000e6);
        verifyBalances(maker, btcAddress, 55e8, 45e8, 55e8);

        // taker will buy 2 BTC
        // submit 2 signed orders - taker fee will be 100 USDC and maker fee will be 20 USDC
        (IExchange.OrderWithSignature memory takerOrder, bytes32 takerOrderDigest) =
            signOrder(takerPrivateKey, btcAddress, usdcAddress, 2e8, 0, 1);
        (IExchange.OrderWithSignature memory makerOrder, bytes32 makerOrderDigest) =
            signOrder(makerPrivateKey, btcAddress, usdcAddress, -2e8, 70000e6, 2);
        bytes memory tx1 =
            createTradeExecution(btcAddress, usdcAddress, 2e8, 70000e6, 100e6, 20e6, takerOrder, makerOrder);

        bytes[] memory txs = new bytes[](1);
        txs[0] = tx1;
        vm.prank(submitter);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            takerOrderDigest,
            takerOrder.tx.sender,
            btcAddress,
            usdcAddress,
            true,
            takerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: 2e8,
                executionPrice: 70000e6,
                fee: 100e6,
                baseAdjustment: 2e8,
                quoteAdjustment: -(140000e6 + 100e6)
            })
        );
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            makerOrderDigest,
            makerOrder.tx.sender,
            btcAddress,
            usdcAddress,
            false,
            makerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: -2e8,
                executionPrice: 70000e6,
                fee: 20e6,
                baseAdjustment: -2e8,
                quoteAdjustment: 140000e6 - 20e6
            })
        );
        exchange.submitTransactions(txs);

        verifyBalances(taker, btcAddress, 2e8, 100e8, 55e8);
        verifyBalances(maker, btcAddress, 55e8 - 2e8, 45e8, 55e8);
        verifyBalances(taker, usdcAddress, 200000e6 - 140000e6 - 100e6, 300000e6, 200000e6);
        verifyBalances(maker, usdcAddress, 140000e6 - 20e6, 500000e6, 200000e6);
        verifyBalances(feeAccount, usdcAddress, 100e6 + 20e6, 0, 200000e6);
    }

    function test_SingleSellTrade_BTC_USDC() public {
        setupWallets();

        deposit(taker, btcAddress, 5e8);
        verifyBalances(taker, usdcAddress, 0, 500000e6, 0);
        verifyBalances(taker, btcAddress, 5e8, 95e8, 5e8);
        deposit(maker, usdcAddress, 200000e6);
        verifyBalances(maker, usdcAddress, 200000e6, 300000e6, 200000e6);
        verifyBalances(maker, btcAddress, 0, 100e8, 5e8);

        // taker will sell 2 BTC, for a price of 70000 USDC per BTC
        // submit 2 signed orders - taker fee will be 100 USDC and maker fee will be 20 USDC
        (IExchange.OrderWithSignature memory takerOrder, bytes32 takerOrderDigest) =
            signOrder(takerPrivateKey, btcAddress, usdcAddress, -2e8, 0, 1);
        (IExchange.OrderWithSignature memory makerOrder, bytes32 makerOrderDigest) =
            signOrder(makerPrivateKey, btcAddress, usdcAddress, 5e8, 70000e6, 2);
        bytes memory tx1 =
            createTradeExecution(btcAddress, usdcAddress, -2e8, 70000e6, 100e6, 20e6, takerOrder, makerOrder);

        bytes[] memory txs = new bytes[](1);
        txs[0] = tx1;
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            takerOrderDigest,
            takerOrder.tx.sender,
            btcAddress,
            usdcAddress,
            true,
            takerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: -2e8,
                executionPrice: 70000e6,
                fee: 100e6,
                baseAdjustment: -2e8,
                quoteAdjustment: 140000e6 - 100e6
            })
        );
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            makerOrderDigest,
            makerOrder.tx.sender,
            btcAddress,
            usdcAddress,
            false,
            makerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: 2e8,
                executionPrice: 70000e6,
                fee: 20e6,
                baseAdjustment: 2e8,
                quoteAdjustment: -(140000e6 + 20e6)
            })
        );
        vm.prank(submitter);
        exchange.submitTransactions(txs);

        verifyBalances(taker, btcAddress, 3e8, 95e8, 5e8);
        verifyBalances(maker, btcAddress, 2e8, 100e8, 5e8);
        verifyBalances(taker, usdcAddress, 140000e6 - 100e6, 500000e6, 200000e6);
        verifyBalances(maker, usdcAddress, 200000e6 - 140000e6 - 20e6, 300000e6, 200000e6);
        verifyBalances(feeAccount, usdcAddress, 100e6 + 20e6, 0, 200000e6);
    }

    function test_NativeTrade_BTC_ETH() public {
        setupWallets();

        deposit(taker, 3e18);
        verifyBalances(taker, btcAddress, 0, 100e8, 0);
        verifyBalances(taker, 3e18, 7e18, 3e18);
        deposit(maker, btcAddress, 2e8);
        verifyBalances(maker, btcAddress, 2e8, 98e8, 2e8);
        verifyBalances(maker, 0, 10e18, 3e18);

        // taker will buy .1 BTC, price is 20 ETH per BTC so will need to pay 2ETH, takerFee will be .02 ETH and makerFee will be 0.01 ETH
        (IExchange.OrderWithSignature memory takerOrder, bytes32 takerOrderDigest) =
            signOrder(takerPrivateKey, btcAddress, address(0), 1e7, 20e18, 1);
        (IExchange.OrderWithSignature memory makerOrder, bytes32 makerOrderDigest) =
            signOrder(makerPrivateKey, btcAddress, address(0), 2e8, 20e18, 2);
        bytes memory tx1 = createTradeExecution(btcAddress, address(0), 1e7, 20e18, 2e16, 1e16, takerOrder, makerOrder);

        bytes[] memory txs = new bytes[](1);
        txs[0] = tx1;
        vm.prank(submitter);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            takerOrderDigest,
            takerOrder.tx.sender,
            btcAddress,
            address(0),
            true,
            takerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: 1e7,
                executionPrice: 20e18,
                fee: 2e16,
                baseAdjustment: 1e7,
                quoteAdjustment: -(2e18 + 2e16)
            })
        );
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            makerOrderDigest,
            makerOrder.tx.sender,
            btcAddress,
            address(0),
            false,
            makerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: -1e7,
                executionPrice: 20e18,
                fee: 1e16,
                baseAdjustment: -1e7,
                quoteAdjustment: 2e18 - 1e16
            })
        );
        exchange.submitTransactions(txs);

        verifyBalances(taker, btcAddress, 1e7, 100e8, 2e8);
        verifyBalances(taker, 3e18 - 2e18 - 2e16, 7e18, 3e18);
        verifyBalances(maker, btcAddress, 2e8 - 1e7, 98e8, 2e8);
        verifyBalances(maker, 2e18 - 1e16, 10e18, 3e18);
        verifyBalances(feeAccount, 3e16, 0, 3e18);
    }

    function test_MultipleTrades_BTC_USDC() public {
        setupWallets();

        deposit(taker, btcAddress, 5e8);
        verifyBalances(taker, usdcAddress, 0, 500000e6, 0);
        verifyBalances(taker, btcAddress, 5e8, 95e8, 5e8);
        deposit(maker, usdcAddress, 200000e6);
        verifyBalances(maker, usdcAddress, 200000e6, 300000e6, 200000e6);
        verifyBalances(maker, btcAddress, 0, 100e8, 5e8);

        // taker will sell 2 BTC, for a price of 70000 USDC per BTC
        // it will match against 2 orders from maker
        (IExchange.OrderWithSignature memory takerOrder, bytes32 takerOrderDigest) =
            signOrder(takerPrivateKey, btcAddress, usdcAddress, -2e8, 70000e6, 1);
        (IExchange.OrderWithSignature memory makerOrder1, bytes32 makerOrderDigest1) =
            signOrder(makerPrivateKey, btcAddress, usdcAddress, 6e7, 70000e6, 2);
        (IExchange.OrderWithSignature memory makerOrder2, bytes32 makerOrderDigest2) =
            signOrder(makerPrivateKey, btcAddress, usdcAddress, 14e7, 70000e6, 3);
        bytes memory tx1 =
            createTradeExecution(btcAddress, usdcAddress, -6e7, 70000e6, 40e6, 10e6, takerOrder, makerOrder1);
        bytes memory tx2 =
            createTradeExecution(btcAddress, usdcAddress, -14e7, 70000e6, 60e6, 10e6, takerOrder, makerOrder2);

        bytes[] memory txs = new bytes[](2);
        txs[0] = tx1;
        txs[1] = tx2;
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            takerOrderDigest,
            takerOrder.tx.sender,
            btcAddress,
            usdcAddress,
            true,
            takerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: -6e7,
                executionPrice: 70000e6,
                fee: 40e6,
                baseAdjustment: -6e7,
                quoteAdjustment: 42000e6 - 40e6
            })
        );
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            makerOrderDigest1,
            makerOrder1.tx.sender,
            btcAddress,
            usdcAddress,
            false,
            makerOrder1.tx,
            IExchange.ExecutionInfo({
                filledAmount: 6e7,
                executionPrice: 70000e6,
                fee: 10e6,
                baseAdjustment: 6e7,
                quoteAdjustment: -(42000e6 + 10e6)
            })
        );
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            takerOrderDigest,
            takerOrder.tx.sender,
            btcAddress,
            usdcAddress,
            true,
            takerOrder.tx,
            IExchange.ExecutionInfo({
                filledAmount: -14e7,
                executionPrice: 70000e6,
                fee: 60e6,
                baseAdjustment: -14e7,
                quoteAdjustment: 98000e6 - 60e6
            })
        );
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.OrderFilled(
            makerOrderDigest2,
            makerOrder2.tx.sender,
            btcAddress,
            usdcAddress,
            false,
            makerOrder2.tx,
            IExchange.ExecutionInfo({
                filledAmount: 14e7,
                executionPrice: 70000e6,
                fee: 10e6,
                baseAdjustment: 14e7,
                quoteAdjustment: -(98000e6 + 10e6)
            })
        );
        vm.prank(submitter);
        exchange.submitTransactions(txs);

        verifyBalances(taker, btcAddress, 3e8, 95e8, 5e8);
        verifyBalances(maker, btcAddress, 2e8, 100e8, 5e8);
        verifyBalances(taker, usdcAddress, 139900e6, 500000e6, 200000e6);
        verifyBalances(maker, usdcAddress, 59980e6, 300000e6, 200000e6);
        verifyBalances(feeAccount, usdcAddress, 120e6, 0, 200000e6);
    }

    function test_TradeErrors() public {
        setupWallets();

        deposit(taker, btcAddress, 5e8);
        verifyBalances(taker, usdcAddress, 0, 500000e6, 0);
        verifyBalances(taker, btcAddress, 5e8, 95e8, 5e8);
        deposit(maker, usdcAddress, 200000e6);
        verifyBalances(maker, usdcAddress, 200000e6, 300000e6, 200000e6);
        verifyBalances(maker, btcAddress, 0, 100e8, 5e8);

        {
            // buy more than taker has for quote currency
            (IExchange.OrderWithSignature memory takerOrder,) =
                signOrder(takerPrivateKey, btcAddress, usdcAddress, 3e8, 70000e6, 1);
            (IExchange.OrderWithSignature memory makerOrder,) =
                signOrder(makerPrivateKey, btcAddress, usdcAddress, -5e8, 70000e6, 2);
            bytes memory tx1 = createTradeExecution(btcAddress, address(0), 3e8, 70000e6, 0, 0, takerOrder, makerOrder);

            bytes[] memory txs = new bytes[](1);
            txs[0] = tx1;
            vm.prank(submitter);
            vm.expectEmit(exchangeProxyAddress);
            emit IExchange.AmountAdjusted(maker, btcAddress, 3e8, 0);
            vm.expectEmit(exchangeProxyAddress);
            emit IExchange.AmountAdjusted(taker, address(0), 210000e6, 0);
            exchange.submitTransactions(txs);
        }

        {
            // sell more than taker has for quote currency
            (IExchange.OrderWithSignature memory takerOrder,) =
                signOrder(takerPrivateKey, btcAddress, usdcAddress, -6e8, 70000e6, 1);
            (IExchange.OrderWithSignature memory makerOrder,) =
                signOrder(makerPrivateKey, btcAddress, usdcAddress, 8e8, 70000e6, 2);
            bytes memory tx1 = createTradeExecution(btcAddress, address(0), -6e8, 70000e6, 0, 0, takerOrder, makerOrder);

            bytes[] memory txs = new bytes[](1);
            txs[0] = tx1;
            vm.prank(submitter);
            vm.expectEmit(exchangeProxyAddress);
            emit IExchange.AmountAdjusted(maker, address(0), 420000e6, 210000e6);
            exchange.submitTransactions(txs);
        }
    }

    function signOrder(
        uint256 privateKey,
        address baseToken,
        address quoteToken,
        int256 amount,
        uint256 price,
        uint256 nonce
    ) internal view returns (IExchange.OrderWithSignature memory, bytes32) {
        IExchange.Order memory order =
            IExchange.Order({sender: vm.addr(privateKey), amount: amount, price: price, nonce: nonce});
        bytes32 digest =
            SigUtils.getTypedDataHash(exchange.DOMAIN_SEPARATOR(), SigUtils.getStructHash(baseToken, quoteToken, order));
        return (IExchange.OrderWithSignature(order, sign(privateKey, digest)), digest);
    }

    function createTradeExecution(
        address baseToken,
        address quoteToken,
        int256 amount,
        uint256 price,
        uint256 takerFee,
        uint256 makerFee,
        IExchange.OrderWithSignature memory takerOrder,
        IExchange.OrderWithSignature memory makerOrder
    ) internal pure returns (bytes memory) {
        return packTx(
            IExchange.TransactionType.SettleTrade,
            abi.encodePacked(
                abi.encode(baseToken, quoteToken, amount, price, takerFee, makerFee),
                uint256(0x100),
                uint256(0x220),
                abi.encode(
                    takerOrder.tx.sender,
                    takerOrder.tx.amount,
                    takerOrder.tx.price,
                    takerOrder.tx.nonce,
                    takerOrder.signature
                ),
                abi.encode(
                    makerOrder.tx.sender,
                    makerOrder.tx.amount,
                    makerOrder.tx.price,
                    makerOrder.tx.nonce,
                    makerOrder.signature
                )
            )
        );
    }

    function setupWallets() internal {
        MockERC20 usdcMock = new MockERC20("USD Coin", "USDC", 6);
        usdcMock.mint(taker, 500000e6);
        usdcMock.mint(maker, 500000e6);
        MockERC20 btcMock = new MockERC20("Bitcoin", "BTC", 8);
        btcMock.mint(taker, 100e8);
        btcMock.mint(maker, 100e8);

        usdcAddress = address(usdcMock);
        btcAddress = address(btcMock);
    }
}
