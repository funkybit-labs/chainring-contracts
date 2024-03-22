// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "./common/Constants.sol";
import "./interfaces/IVersion.sol";
import "./interfaces/IExchange.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {Initializable} from "openzeppelin-contracts/contracts/proxy/utils/Initializable.sol";
import {OwnableUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/access/OwnableUpgradeable.sol";
import {UUPSUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/proxy/utils/UUPSUpgradeable.sol";
import {EIP712Upgradeable} from "openzeppelin-contracts-upgradeable/contracts/utils/cryptography/EIP712Upgradeable.sol";
import {ECDSA} from "openzeppelin-contracts/contracts/utils/cryptography/ECDSA.sol";

contract Exchange is EIP712Upgradeable, UUPSUpgradeable, OwnableUpgradeable, IExchange {
    mapping(address => mapping(address => uint256)) public balances;
    mapping(address => uint256) public nativeBalances;
    mapping(address => uint64) public nonces;

    uint256 public txProcessedCount;
    address public submitter;

    string constant WITHDRAW_SIGNATURE = "Withdraw(address sender,address token,uint256 amount,uint64 nonce)";
    string constant WITHDRAW_NATIVE_SIGNATURE = "Withdraw(address sender,uint256 amount,uint64 nonce)";

    function initialize(address _submitter) public initializer {
        __Ownable_init(msg.sender);
        __UUPSUpgradeable_init();
        __EIP712_init("ChainRing Labs", "0.0.1");
        submitter = _submitter;
    }

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}

    function getVersion() external pure returns (uint64) {
        return VERSION;
    }

    function DOMAIN_SEPARATOR() external view returns (bytes32) {
        return _domainSeparatorV4();
    }

    function setSubmitter(address _submitter) external onlyOwner {
        require(_submitter != address(0), "Not a valid address");
        submitter = _submitter;
    }

    function deposit(address _token, uint256 _amount) external {
        IERC20 erc20 = IERC20(_token);
        erc20.transferFrom(msg.sender, address(this), _amount);

        balances[msg.sender][_token] += _amount;
        emit Deposit(msg.sender, _token, _amount);
    }

    receive() external payable {
        nativeBalances[msg.sender] += msg.value;
        emit Deposit(msg.sender, address(0), msg.value);
    }

    function withdraw(address _token, uint256 _amount) external {
        _withdraw(msg.sender, _token, _amount);
    }

    function withdraw(uint256 _amount) external {
        _withdrawNative(msg.sender, _amount);
    }

    function submitTransactions(bytes[] calldata transactions) public onlySubmitter {
        for (uint256 i = 0; i < transactions.length; i++) {
            bytes calldata transaction = transactions[i];
            processTransaction(transaction);
        }
        txProcessedCount += transactions.length;
    }

    function processTransaction(bytes calldata transaction) internal {
        TransactionType txType = TransactionType(uint8(transaction[0]));
        if (txType == TransactionType.Withdraw) {
            WithdrawWithSignature memory signedTx = abi.decode(transaction[1:], (WithdrawWithSignature));
            validateNonce(signedTx.tx.sender, signedTx.tx.nonce);
            bytes32 digest = _hashTypedDataV4(
                keccak256(
                    abi.encode(
                        keccak256(bytes(WITHDRAW_SIGNATURE)),
                        signedTx.tx.sender,
                        signedTx.tx.token,
                        signedTx.tx.amount,
                        signedTx.tx.nonce
                    )
                )
            );
            validateSignature(signedTx.tx.sender, digest, signedTx.signature);
            _withdraw(signedTx.tx.sender, signedTx.tx.token, signedTx.tx.amount);
        } else if (txType == TransactionType.WithdrawNative) {
            WithdrawNativeWithSignature memory signedTx = abi.decode(transaction[1:], (WithdrawNativeWithSignature));
            validateNonce(signedTx.tx.sender, signedTx.tx.nonce);
            bytes32 digest = _hashTypedDataV4(
                keccak256(
                    abi.encode(
                        keccak256(bytes(WITHDRAW_NATIVE_SIGNATURE)),
                        signedTx.tx.sender,
                        signedTx.tx.amount,
                        signedTx.tx.nonce
                    )
                )
            );
            validateSignature(signedTx.tx.sender, digest, signedTx.signature);
            _withdrawNative(signedTx.tx.sender, signedTx.tx.amount);
        }
    }

    modifier onlySubmitter() {
        require(msg.sender == submitter, "Sender is not the submitter");
        _;
    }

    function validateNonce(address sender, uint64 nonce) internal virtual {
        require(nonce == nonces[sender]++, "Invalid Nonce");
    }

    function validateSignature(address sender, bytes32 digest, bytes memory signature) internal view virtual {
        address recovered = ECDSA.recover(digest, signature);
        require(recovered == sender, "Invalid Signature");
    }

    function _withdraw(address _sender, address _token, uint256 _amount) internal {
        uint256 balance = balances[_sender][_token];
        if (_amount != 0) {
            require(balance >= _amount, "Insufficient Balance");
        } else {
            _amount = balance;
        }

        IERC20 erc20 = IERC20(_token);
        erc20.transfer(_sender, _amount);

        balances[_sender][_token] -= _amount;
        emit Withdrawal(_sender, _token, _amount);
    }

    function _withdrawNative(address _sender, uint256 _amount) internal {
        uint256 balance = nativeBalances[_sender];
        if (_amount != 0) {
            require(balance >= _amount, "Insufficient Balance");
        } else {
            _amount = balance;
        }
        payable(_sender).transfer(_amount);

        nativeBalances[_sender] -= _amount;
        emit Withdrawal(_sender, address(0), _amount);
    }
}
