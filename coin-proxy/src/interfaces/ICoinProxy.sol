// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import "./IVersion.sol";

interface ICoinProxy is IVersion {
    event DepositSucceeded(address indexed from, uint64 sequence, address token, uint256 amount);
    event WithdrawalSucceeded(address indexed to, uint64 sequence, address token, uint256 amount, uint256 fee);
    event WithdrawalRolledBack(address indexed to, uint64 sequence, address token, uint256 amount, uint256 fee);
    event WithdrawalFailed(
        address indexed _address, uint64 sequence, address token, uint256 amount, uint256 balance, ErrorCode errorCode
    );
    event SettlementFailed(
        address indexed _address, address token, bytes32[] tradeHashes, uint256 requestedAmount, uint256 balance
    );
    event SettlementCompleted(address indexed _address, bytes32[] tradeHashes);

    error ErrorDidNotNetToZero(address token);

    enum ErrorCode {
        InsufficientBalance,
        InsufficientFeeBalance
    }

    enum TransactionType {
        Withdraw,
        WithdrawAll
    }

    struct Deposit {
        uint64 sequence;
        address sender;
        address token;
        uint256 amount;
    }

    struct BatchDeposit {
        Deposit[] deposits;
    }

    struct Withdrawal {
        address sender;
        address token;
        uint256 amount;
        uint64 sequence;
        uint256 feeAmount;
    }

    struct BatchWithdrawal {
        Withdrawal[] withdrawals;
    }

    struct Adjustment {
        uint16 walletIndex;
        uint256 amount;
    }

    struct WalletTradeList {
        bytes32[] tradeHashes;
    }

    struct TokenAdjustmentList {
        address token;
        Adjustment[] increments;
        Adjustment[] decrements;
        uint256 feeAmount;
    }

    struct BatchSettlement {
        address[] walletAddresses;
        WalletTradeList[] walletTradeLists;
        TokenAdjustmentList[] tokenAdjustmentLists;
    }

    function DOMAIN_SEPARATOR() external view returns (bytes32);

    function submitDeposits(bytes calldata data) external;

    function submitSettlementBatch(bytes calldata data) external;

    function prepareSettlementBatch(bytes calldata data) external;

    function rollbackBatch() external;

    function submitWithdrawalBatch(bytes calldata data) external;

    function rollbackWithdrawalBatch(bytes calldata data) external;

    function setSubmitter(address _submitter) external;

    function setFeeAccount(address _feeAccount) external;
}
