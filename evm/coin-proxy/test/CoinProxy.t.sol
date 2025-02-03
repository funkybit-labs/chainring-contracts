// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {CoinProxy} from "../src/CoinProxy.sol";
import {ICoinProxy} from "../src/interfaces/ICoinProxy.sol";
import {ERC1967Utils} from "openzeppelin-contracts/contracts/proxy/ERC1967/ERC1967Utils.sol";
import "./contracts/CoinProxyUpgrade.sol";
import "./CoinProxyBaseTest.sol";

contract CoinProxyTest is CoinProxyBaseTest {
    function setUp() public override {
        super.setUp();
        vm.deal(wallet1, 10 ether);
        vm.deal(wallet2, 10 ether);
    }

    function test_Deposit() public {
        setupWallets();

        deposit(wallet1, coinAddress, 1000e6, 1);
        verifyBalance(wallet1, coinAddress, 1000e6);
        deposit(wallet1, btcAddress, 55e8, 2);
        verifyBalance(wallet1, btcAddress, 55e8);
    }

    function test_MultipleDeposits() public {
        setupWallets();

        deposit(wallet1, coinAddress, 1000e6, 1);
        verifyBalance(wallet1, coinAddress, 1000e6);
        deposit(wallet1, coinAddress, 300e6, 2);
        verifyBalance(wallet1, coinAddress, 1300e6);

        deposit(wallet1, btcAddress, 55e8, 3);
        verifyBalance(wallet1, btcAddress, 55e8);
        deposit(wallet1, btcAddress, 33e8, 4);
        verifyBalance(wallet1, btcAddress, 88e8);
    }

    function test_Withdrawal() public {
        setupWallets();

        deposit(wallet1, btcAddress, 10e8, 1);
        verifyBalance(wallet1, btcAddress, 10e8);
        deposit(wallet1, coinAddress, 1000e6, 2);
        verifyBalance(wallet1, coinAddress, 1000e6);
        withdraw(wallet1, coinAddress, 133e6, 133e6, 1e8, 1);
        verifyBalance(wallet1, coinAddress, 867e6);
        verifyBalance(wallet1, btcAddress, 9e8);
        verifyBalance(feeAccount, btcAddress, 1e8);

        withdraw(wallet1, btcAddress, 5e8, 5e8, 1e8, 2);
        verifyBalance(wallet1, coinAddress, 867e6);
        verifyBalance(wallet1, btcAddress, 4e8);
        verifyBalance(feeAccount, btcAddress, 2e8);
    }

    function test_Upgrade() public {
        setupWallets();

        deposit(wallet1, btcAddress, 10e8, 1);
        verifyBalance(wallet1, btcAddress, 10e8);
        deposit(wallet1, coinAddress, 1000e6, 2);
        verifyBalance(wallet1, coinAddress, 1000e6);

        // test that only the owner can upgrade the contract
        vm.startPrank(wallet1);
        CoinProxyUpgrade newImplementation = new CoinProxyUpgrade();
        vm.expectRevert(abi.encodeWithSelector(OwnableUnauthorizedAccount.selector, wallet1));
        coinProxy.upgradeToAndCall(address(newImplementation), "");
        vm.stopPrank();

        // call the proxy this time as the owner to perform the upgrade and also send data to invoke a function
        // in the new implementation as part of the upgrade
        // we should verify we get an Upgraded event with the implementation contract address from the proxy
        vm.startPrank(coinProxy.owner());
        vm.expectEmit(coinProxyProxyAddress);
        emit ERC1967Utils.Upgraded(address(newImplementation));
        coinProxy.upgradeToAndCall(
            address(newImplementation), abi.encodeWithSelector(CoinProxyUpgrade.setValue.selector, 1000)
        );
        vm.stopPrank();
        // verify the new value in the upgraded contract is set as part of the upgrade
        assertEq(CoinProxyUpgrade(coinProxyProxyAddress).value(), 1000);

        // check balances are maintained after the upgrade
        verifyBalance(wallet1, btcAddress, 10e8);
        verifyBalance(wallet1, coinAddress, 1000e6);

        // perform some withdrawals
        withdraw(wallet1, coinAddress, 133e6, 133e6, 1e8, 1);
        verifyBalance(wallet1, coinAddress, 867e6);
        verifyBalance(wallet1, btcAddress, 9e8);
        verifyBalance(feeAccount, btcAddress, 1e8);

        withdraw(wallet1, btcAddress, 5e8, 5e8, 1e8, 2);
        verifyBalance(wallet1, coinAddress, 867e6);
        verifyBalance(wallet1, btcAddress, 4e8);
        verifyBalance(feeAccount, btcAddress, 2e8);
    }

    function test_WithdrawInsufficientBalance() public {
        setupWallets();

        deposit(wallet1, btcAddress, 10e8, 1);
        verifyBalance(wallet1, btcAddress, 10e8);
        deposit(wallet1, coinAddress, 1000e6, 2);
        verifyBalance(wallet1, coinAddress, 1000e6);

        withdraw(wallet1, coinAddress, 2000e6, 1000e6, 1e8, 1);
        verifyBalance(wallet1, coinAddress, 1000e6);

        withdrawInsufficientFee(wallet1, coinAddress, 1000e6, 10e8, 12e8, 2);
        verifyBalance(wallet1, coinAddress, 1000e6);
    }

    function test_Batches() public {
        setupWallets();

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.DepositSucceeded(wallet1, 1, btcAddress, 5e8);
        emit ICoinProxy.DepositSucceeded(wallet1, 2, coinAddress, 1000e6);
        emit ICoinProxy.DepositSucceeded(wallet1, 3, btcAddress, 12e8);
        emit ICoinProxy.DepositSucceeded(wallet1, 4, coinAddress, 2000e6);
        ICoinProxy.BatchDeposit memory depositBatch = ICoinProxy.BatchDeposit(new ICoinProxy.Deposit[](4));
        depositBatch.deposits[0] = ICoinProxy.Deposit({sequence: 1, sender: wallet1, token: btcAddress, amount: 5e8});
        depositBatch.deposits[1] =
            ICoinProxy.Deposit({sequence: 2, sender: wallet1, token: coinAddress, amount: 1000e6});
        depositBatch.deposits[2] = ICoinProxy.Deposit({sequence: 3, sender: wallet1, token: btcAddress, amount: 12e8});
        depositBatch.deposits[3] =
            ICoinProxy.Deposit({sequence: 4, sender: wallet1, token: coinAddress, amount: 2000e6});
        coinProxy.submitDeposits(abi.encode(depositBatch));
        vm.stopPrank();

        verifyBalance(wallet1, btcAddress, 17e8);
        verifyBalance(wallet1, coinAddress, 3000e6);
        verifyBalance(feeAccount, btcAddress, 0);

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 1, btcAddress, 4e8, 1e8);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 2, coinAddress, 1000e6, 1e8);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 3, btcAddress, 8e8, 1e8);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 4, coinAddress, 2000e6, 1e8);
        ICoinProxy.BatchWithdrawal memory withdrawalBatch = ICoinProxy.BatchWithdrawal(new ICoinProxy.Withdrawal[](4));
        withdrawalBatch.withdrawals[0] =
            ICoinProxy.Withdrawal({sequence: 1, sender: wallet1, token: btcAddress, amount: 4e8, feeAmount: 1e8});
        withdrawalBatch.withdrawals[1] =
            ICoinProxy.Withdrawal({sequence: 2, sender: wallet1, token: coinAddress, amount: 1000e6, feeAmount: 1e8});
        withdrawalBatch.withdrawals[2] =
            ICoinProxy.Withdrawal({sequence: 3, sender: wallet1, token: btcAddress, amount: 8e8, feeAmount: 1e8});
        withdrawalBatch.withdrawals[3] =
            ICoinProxy.Withdrawal({sequence: 4, sender: wallet1, token: coinAddress, amount: 2000e6, feeAmount: 1e8});
        coinProxy.submitWithdrawalBatch(abi.encode(withdrawalBatch));
        vm.stopPrank();

        verifyBalance(wallet1, btcAddress, 3e8);
        verifyBalance(wallet1, coinAddress, 0);
        verifyBalance(feeAccount, btcAddress, 4e8);

        // now rollback
        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.WithdrawalRolledBack(wallet1, 1, btcAddress, 4e8, 1e8);
        emit ICoinProxy.WithdrawalRolledBack(wallet1, 2, coinAddress, 1000e6, 1e8);
        emit ICoinProxy.WithdrawalRolledBack(wallet1, 3, btcAddress, 8e8, 1e8);
        emit ICoinProxy.WithdrawalRolledBack(wallet1, 4, coinAddress, 2000e6, 1e8);
        coinProxy.rollbackWithdrawalBatch(abi.encode(withdrawalBatch));

        vm.stopPrank();

        verifyBalance(wallet1, btcAddress, 17e8);
        verifyBalance(wallet1, coinAddress, 3000e6);
        verifyBalance(feeAccount, btcAddress, 0);
    }

    function test_Batches_SomeWithdrawalsFail() public {
        setupWallets();

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.DepositSucceeded(wallet1, 1, btcAddress, 5e8);
        emit ICoinProxy.DepositSucceeded(wallet1, 2, coinAddress, 1000e6);
        emit ICoinProxy.DepositSucceeded(wallet1, 3, btcAddress, 12e8);
        emit ICoinProxy.DepositSucceeded(wallet1, 4, coinAddress, 2000e6);
        ICoinProxy.BatchDeposit memory depositBatch = ICoinProxy.BatchDeposit(new ICoinProxy.Deposit[](4));
        depositBatch.deposits[0] = ICoinProxy.Deposit({sequence: 1, sender: wallet1, token: btcAddress, amount: 5e8});
        depositBatch.deposits[1] =
            ICoinProxy.Deposit({sequence: 2, sender: wallet1, token: coinAddress, amount: 1000e6});
        depositBatch.deposits[2] = ICoinProxy.Deposit({sequence: 3, sender: wallet1, token: btcAddress, amount: 12e8});
        depositBatch.deposits[3] =
            ICoinProxy.Deposit({sequence: 4, sender: wallet1, token: coinAddress, amount: 2000e6});
        coinProxy.submitDeposits(abi.encode(depositBatch));
        vm.stopPrank();

        verifyBalance(wallet1, btcAddress, 17e8);
        verifyBalance(wallet1, coinAddress, 3000e6);
        verifyBalance(feeAccount, btcAddress, 0);

        vm.startPrank(submitter);
        vm.expectEmit(coinProxyProxyAddress);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 1, btcAddress, 4e8, 1e8);
        emit ICoinProxy.WithdrawalFailed(wallet1, 2, coinAddress, 5000e6, 1e8, ICoinProxy.ErrorCode.InsufficientBalance);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 3, btcAddress, 8e8, 1e8);
        emit ICoinProxy.WithdrawalSucceeded(wallet1, 4, coinAddress, 2000e6, 1e8);
        ICoinProxy.BatchWithdrawal memory withdrawalBatch = ICoinProxy.BatchWithdrawal(new ICoinProxy.Withdrawal[](4));
        withdrawalBatch.withdrawals[0] =
            ICoinProxy.Withdrawal({sequence: 1, sender: wallet1, token: btcAddress, amount: 4e8, feeAmount: 1e8});
        withdrawalBatch.withdrawals[1] =
            ICoinProxy.Withdrawal({sequence: 2, sender: wallet1, token: coinAddress, amount: 5000e6, feeAmount: 1e8});
        withdrawalBatch.withdrawals[2] =
            ICoinProxy.Withdrawal({sequence: 3, sender: wallet1, token: btcAddress, amount: 8e8, feeAmount: 1e8});
        withdrawalBatch.withdrawals[3] =
            ICoinProxy.Withdrawal({sequence: 4, sender: wallet1, token: coinAddress, amount: 2000e6, feeAmount: 1e8});
        coinProxy.submitWithdrawalBatch(abi.encode(withdrawalBatch));
        vm.stopPrank();

        verifyBalance(wallet1, btcAddress, 4e8);
        verifyBalance(wallet1, coinAddress, 1000e6);
        verifyBalance(feeAccount, btcAddress, 3e8);
    }
}
