// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import "./IVersion.sol";

interface IExchange is IVersion {
    event Deposit(address indexed from, address token, uint256 amount);
    event Withdrawal(address indexed to, address token, uint256 amount);

    enum TransactionType {
        Withdraw,
        WithdrawNative
    }

    struct Withdraw {
        address sender;
        address token;
        uint256 amount;
        uint64 nonce;
    }

    struct WithdrawWithSignature {
        Withdraw tx;
        bytes signature;
    }

    struct WithdrawNative {
        address sender;
        uint256 amount;
        uint64 nonce;
    }

    struct WithdrawNativeWithSignature {
        WithdrawNative tx;
        bytes signature;
    }

    function DOMAIN_SEPARATOR() external view returns (bytes32);

    function deposit(address _token, uint256 _amount) external;

    receive() external payable;

    function withdraw(address _token, uint256 _amount) external;

    function withdraw(uint256 _amount) external;

    function submitTransactions(bytes[] calldata transactions) external;

    function setSubmitter(address _submitter) external;
}
