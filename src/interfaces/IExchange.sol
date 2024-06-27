// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import "./IVersion.sol";

interface IExchange is IVersion {
    event Deposit(address indexed from, address token, uint256 amount);
    event Withdrawal(address indexed to, uint64 sequence, address token, uint256 amount, uint256 fee);

    error ErrorDidNotNetToZero(address token);

    enum ErrorCode {
        InvalidSignature,
        InsufficientBalance
    }

    enum TransactionType {
        Withdraw,
        WithdrawAll
    }

    struct Withdraw {
        address sender;
        address token;
        uint256 amount;
        uint64 nonce;
        uint256 feeAmount;
    }

    struct WithdrawWithSignature {
        uint64 sequence;
        Withdraw tx;
        bytes signature;
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

    event WithdrawalFailed(
        address indexed _address, uint64 sequence, address token, uint256 amount, uint256 balance, ErrorCode errorCode
    );

    event SettlementFailed(address indexed _address, bytes32[] tradeHashes, uint256 requestedAmount, uint256 balance);

    event SettlementCompleted(address indexed _address, bytes32[] tradeHashes);

    function DOMAIN_SEPARATOR() external view returns (bytes32);

    function deposit(address _token, uint256 _amount) external;

    receive() external payable;

    function submitWithdrawals(bytes[] calldata withdrawals) external;

    function submitSettlementBatch(bytes calldata data) external;

    function prepareSettlementBatch(bytes calldata data) external;

    function rollbackBatch() external;

    function setSubmitter(address _submitter) external;

    function setFeeAccount(address _feeAccount) external;
}
