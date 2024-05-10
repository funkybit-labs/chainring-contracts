// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {MockERC20} from "./contracts/MockERC20.sol";
import {Exchange} from "../src/Exchange.sol";
import {IExchange} from "../src/interfaces/IExchange.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./utils/SigUtils.sol";
import "./contracts/ExchangeUpgrade.sol";
import "./ExchangeBaseTest.sol";

contract ExchangeTest is ExchangeBaseTest {
    function setUp() public override {
        super.setUp();
        vm.deal(wallet1, 10 ether);
        vm.deal(wallet2, 10 ether);
    }

    function test_ERC20Deposit() public {
        setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);
    }

    function test_MultipleERC20Deposits() public {
        setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        deposit(wallet1, usdcAddress, 300e6);
        verifyBalances(wallet1, usdcAddress, 1300e6, 498700e6, 1300e6);

        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);
        deposit(wallet1, btcAddress, 33e8);
        verifyBalances(wallet1, btcAddress, 88e8, 12e8, 88e8);
    }

    function test_ERC20Withdrawal() public {
        setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        withdraw(wallet1PrivateKey, usdcAddress, 133e6, 133e6);
        verifyBalances(wallet1, usdcAddress, 867e6, 499133e6, 867e6);

        deposit(wallet1, btcAddress, 55e8);
        verifyBalances(wallet1, btcAddress, 55e8, 45e8, 55e8);
        withdraw(wallet1PrivateKey, btcAddress, 4e8, 4e8);
        verifyBalances(wallet1, btcAddress, 51e8, 49e8, 51e8);

        withdraw(wallet1PrivateKey, btcAddress, 0, 51e8);
        verifyBalances(wallet1, btcAddress, 0, 100e8, 0);
    }

    function test_MultipleWallets() public {
        setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        deposit(wallet2, usdcAddress, 800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 499200e6, 1800e6);

        withdraw(wallet1PrivateKey, usdcAddress, 133e6, 133e6);
        verifyBalances(wallet1, usdcAddress, 867e6, 499133e6, 1667e6);
        withdraw(wallet2PrivateKey, usdcAddress, 120e6, 120e6);
        verifyBalances(wallet2, usdcAddress, 680e6, 499320e6, 1547e6);
    }

    function test_Upgrade() public {
        setupWallets();

        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        deposit(wallet2, usdcAddress, 800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 499200e6, 1800e6);

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
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1800e6);
        verifyBalances(wallet2, usdcAddress, 800e6, 499200e6, 1800e6);

        // perform some withdrawals
        withdraw(wallet1PrivateKey, usdcAddress, 100e6, 100e6);
        verifyBalances(wallet1, usdcAddress, 900e6, 499100e6, 1700e6);
        withdraw(wallet2PrivateKey, usdcAddress, 120e6, 120e6);
        verifyBalances(wallet2, usdcAddress, 680e6, 499320e6, 1580e6);
    }

    function test_NativeDeposits() public {
        deposit(wallet1, 2e18);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        deposit(wallet2, 3e18);
        verifyBalances(wallet2, 3e18, 7e18, 5e18);
    }

    function test_EIP712Withdrawals() public {
        setupWallets();

        // wallet1 - deposit usdc and native token
        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        deposit(wallet1, 2e18);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        // wallet2 - deposit USDC
        deposit(wallet2, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 2000e6);

        uint64 wallet1Nonce = 1000;
        bytes memory tx1 = createSignedWithdrawTx(wallet1PrivateKey, usdcAddress, 200e6, wallet1Nonce, 1);
        bytes memory tx2 = createSignedWithdrawNativeTx(wallet1PrivateKey, 1e18, wallet1Nonce + 200, 2);
        uint64 wallet2Nonce = 10000;
        bytes memory tx3 = createSignedWithdrawTx(wallet2PrivateKey, usdcAddress, 300e6, wallet2Nonce, 3);

        uint256 txProcessedCount = exchange.txProcessedCount();

        bytes[] memory txs = new bytes[](3);
        txs[0] = tx1;
        txs[1] = tx2;
        txs[2] = tx3;

        vm.prank(submitter);
        exchange.prepareBatch(txs);

        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Withdrawal(wallet1, usdcAddress, 200e6);
        emit IExchange.Withdrawal(wallet1, address(0), 1e18);
        emit IExchange.Withdrawal(wallet2, usdcAddress, 300e6);
        vm.prank(submitter);
        exchange.submitBatch(txs);

        // verify balances
        verifyBalances(wallet1, usdcAddress, 800e6, 499200e6, 1500e6);
        verifyBalances(wallet1, 1e18, 9e18, 1e18);
        verifyBalances(wallet2, usdcAddress, 700e6, 499300e6, 1500e6);

        assertEq(txProcessedCount + 3, exchange.txProcessedCount());
    }

    function test_AmountAdjustment() public {
        setupWallets();
        deposit(wallet1, usdcAddress, 1000e6);
        withdraw(wallet1PrivateKey, usdcAddress, 1001e6, 1000e6);
        vm.stopPrank();

        deposit(wallet1, 2e18);
        withdraw(wallet1PrivateKey, address(0), 3e18, 2e18);
        verifyBalances(wallet1, 0, 10e18, 0);
    }

    function test_EIP712ErrorCases() public {
        setupWallets();

        // wallet1 - deposit usdc and native token
        deposit(wallet1, usdcAddress, 1000e6);
        verifyBalances(wallet1, usdcAddress, 1000e6, 499000e6, 1000e6);
        deposit(wallet1, 2e18);
        verifyBalances(wallet1, 2e18, 8e18, 2e18);

        uint64 wallet1Nonce = 22222;
        bytes memory tx1 = createSignedWithdrawTx(wallet1PrivateKey, usdcAddress, 200e6, wallet1Nonce, 1);
        bytes memory tx2 = createSignedWithdrawNativeTx(wallet1PrivateKey, 3e18, wallet1Nonce + 1, 2); // insufficient balance
        uint256 txProcessedCount = exchange.txProcessedCount();

        bytes[] memory txs = new bytes[](2);
        txs[0] = tx1;
        txs[1] = tx2;

        // check fails if not the submitter
        vm.prank(wallet1);
        vm.expectRevert(bytes("Sender is not the submitter"));
        exchange.prepareBatch(txs);

        // must be a valid address
        vm.expectRevert(bytes("Not a valid address"));
        exchange.setSubmitter(address(0));

        // only owner can change the submitter
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
        exchange.submitBatch(txs);

        // check balance adjusted by remaining amounting if too much requested and amount adjusted events emitted
        txs[1] = tx2;
        vm.startPrank(newSubmitter);
        exchange.prepareBatch(txs);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.AmountAdjusted(wallet1, address(0), 3e18, 2e18);
        vm.expectEmit(exchangeProxyAddress);
        emit IExchange.Withdrawal(wallet1, address(0), 2e18);
        exchange.submitBatch(txs);
        vm.stopPrank();

        assertEq(txProcessedCount + 2, exchange.txProcessedCount());

        // set it back
        exchange.setSubmitter(submitter);
    }
}
