// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "./common/Constants.sol";
import "./interfaces/IVersion.sol";
import "./interfaces/IExchange.sol";
import {ERC20} from "openzeppelin-contracts/contracts/token/ERC20/ERC20.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {Initializable} from "openzeppelin-contracts/contracts/proxy/utils/Initializable.sol";
import {OwnableUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/access/OwnableUpgradeable.sol";
import {UUPSUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/proxy/utils/UUPSUpgradeable.sol";
import {EIP712Upgradeable} from "openzeppelin-contracts-upgradeable/contracts/utils/cryptography/EIP712Upgradeable.sol";
import {ECDSA} from "openzeppelin-contracts/contracts/utils/cryptography/ECDSA.sol";

contract Exchange is EIP712Upgradeable, UUPSUpgradeable, OwnableUpgradeable, IExchange {
    mapping(address => mapping(address => uint256)) public balances;
    address public submitter;
    address public feeAccount;
    bytes32 public batchHash;

    string constant WITHDRAW_SIGNATURE = "Withdraw(address sender,address token,uint256 amount,uint64 nonce)";

    function initialize(address _submitter, address _feeAccount) public initializer {
        __Ownable_init(msg.sender);
        __UUPSUpgradeable_init();
        __EIP712_init("ChainRing Labs", "0.0.1");
        submitter = _submitter;
        feeAccount = _feeAccount;
    }

    receive() external payable {
        balances[msg.sender][address(0)] += msg.value;
        emit Deposit(msg.sender, address(0), msg.value);
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

    function setFeeAccount(address _feeAccount) external onlyOwner {
        require(_feeAccount != address(0), "Not a valid address");
        feeAccount = _feeAccount;
    }

    function deposit(address _token, uint256 _amount) external {
        IERC20 erc20 = IERC20(_token);
        erc20.transferFrom(msg.sender, address(this), _amount);

        balances[msg.sender][_token] += _amount;
        emit Deposit(msg.sender, _token, _amount);
    }

    function submitWithdrawals(bytes[] calldata withdrawals) public onlySubmitter {
        require(batchHash == 0, "Settlement batch in process");
        for (uint256 i = 0; i < withdrawals.length; i++) {
            bytes calldata withdrawal = withdrawals[i];
            WithdrawWithSignature memory signedTx = abi.decode(withdrawal, (WithdrawWithSignature));
            if (_validateWithdrawal(signedTx)) {
                _withdraw(signedTx.tx.sender, signedTx.tx.token, signedTx.tx.amount);
            }
        }
    }

    function prepareSettlementBatch(bytes calldata data) public onlySubmitter {
        require(batchHash == 0, "Batch in progress, submit or rollback");
        bool _batchSucceeded = true;

        BatchSettlement memory _batchSettlement = abi.decode(data, (BatchSettlement));

        require(
            _batchSettlement.walletAddresses.length == _batchSettlement.walletTradeLists.length,
            "Invalid address and trade lists lengths"
        );

        // make sure all adjustments net to 0
        for (uint32 i = 0; i < _batchSettlement.tokenAdjustmentLists.length; i++) {
            int256 _netAmount = int256(_batchSettlement.tokenAdjustmentLists[i].feeAmount);
            address _token = _batchSettlement.tokenAdjustmentLists[i].token;
            for (uint32 j = 0; j < _batchSettlement.tokenAdjustmentLists[i].increments.length; j++) {
                _netAmount += int256(_batchSettlement.tokenAdjustmentLists[i].increments[j].amount);
            }
            for (uint32 j = 0; j < _batchSettlement.tokenAdjustmentLists[i].decrements.length; j++) {
                uint256 _adjustmentAmount = _batchSettlement.tokenAdjustmentLists[i].decrements[j].amount;
                _netAmount -= int256(_adjustmentAmount);
                // see if we can apply the decrement
                uint32 walletIndex = _batchSettlement.tokenAdjustmentLists[i].decrements[j].walletIndex;
                address _wallet = _batchSettlement.walletAddresses[walletIndex];
                if (_adjustmentAmount > balances[_wallet][_token]) {
                    _batchSucceeded = false;
                    emit SettlementFailed(
                        _wallet,
                        _batchSettlement.walletTradeLists[walletIndex].tradeHashes,
                        ErrorCode.InsufficientBalance
                    );
                }
            }
            if (_netAmount != 0) {
                revert ErrorDidNotNetToZero(_batchSettlement.tokenAdjustmentLists[i].token);
            }
        }
        if (_batchSucceeded) {
            batchHash = keccak256(data);
        }
    }

    function submitSettlementBatch(bytes calldata data) public onlySubmitter {
        require(batchHash != 0, "No batch prepared");
        require(batchHash == keccak256(data), "Hash does not match prepared batch");

        BatchSettlement memory _batchSettlement = abi.decode(data, (BatchSettlement));
        for (uint32 i = 0; i < _batchSettlement.tokenAdjustmentLists.length; i++) {
            address _token = _batchSettlement.tokenAdjustmentLists[i].token;
            for (uint32 j = 0; j < _batchSettlement.tokenAdjustmentLists[i].increments.length; j++) {
                uint256 _adjustmentAmount = _batchSettlement.tokenAdjustmentLists[i].increments[j].amount;
                address _wallet =
                    _batchSettlement.walletAddresses[_batchSettlement.tokenAdjustmentLists[i].increments[j].walletIndex];
                balances[_wallet][_token] += _adjustmentAmount;
            }
            for (uint32 j = 0; j < _batchSettlement.tokenAdjustmentLists[i].decrements.length; j++) {
                uint256 _adjustmentAmount = _batchSettlement.tokenAdjustmentLists[i].decrements[j].amount;
                address _wallet =
                    _batchSettlement.walletAddresses[_batchSettlement.tokenAdjustmentLists[i].decrements[j].walletIndex];
                if (_adjustmentAmount <= balances[_wallet][_token]) {
                    balances[_wallet][_token] -= _adjustmentAmount;
                } else {
                    revert("Insufficient Balance");
                }
            }
            if (_batchSettlement.tokenAdjustmentLists[i].feeAmount != 0) {
                balances[feeAccount][_token] += _batchSettlement.tokenAdjustmentLists[i].feeAmount;
            }
        }

        for (uint32 i = 0; i < _batchSettlement.walletTradeLists.length; i++) {
            emit IExchange.SettlementCompleted(
                _batchSettlement.walletAddresses[i], _batchSettlement.walletTradeLists[i].tradeHashes
            );
        }

        batchHash = 0;
    }

    function rollbackBatch() external onlySubmitter {
        batchHash = 0;
    }

    modifier onlySubmitter() {
        require(msg.sender == submitter, "Sender is not the submitter");
        _;
    }

    function _calculateBatchHash(bytes[] calldata transactions) internal pure returns (bytes32) {
        bytes memory buffer = new bytes(0);
        for (uint256 i = 0; i < transactions.length; i++) {
            bytes calldata transaction = transactions[i];
            buffer = bytes.concat(buffer, transaction);
        }
        return keccak256(buffer);
    }

    function _validateWithdrawal(WithdrawWithSignature memory signedTx) internal returns (bool) {
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
        return _validateSignature(signedTx.tx.sender, digest, signedTx.signature, signedTx.sequence);
    }

    function _validateSignature(address _sender, bytes32 _digest, bytes memory _signature, uint64 sequence)
        internal
        returns (bool)
    {
        address recovered = ECDSA.recover(_digest, _signature);
        if (recovered != _sender) {
            emit WithdrawalFailed(sequence, ErrorCode.InvalidSignature);
            return false;
        }
        return true;
    }

    function _withdraw(address _sender, address _token, uint256 _amount) internal {
        uint256 balance = balances[_sender][_token];
        if (_amount == 0) {
            _amount = balance;
        }
        uint256 _actual = uint256(-_adjustBalance(_sender, _token, -int256(_amount)));

        if (_token == address(0)) {
            payable(_sender).transfer(_actual);
        } else {
            IERC20 erc20 = IERC20(_token);
            erc20.transfer(_sender, _actual);
        }

        emit Withdrawal(_sender, _token, _actual);
    }

    function _adjustBalance(address _sender, address _token, int256 _amount) internal returns (int256) {
        if (_amount < 0) {
            uint256 balance = balances[_sender][_token];
            uint256 amount = uint256(-_amount);
            if (amount > balance) {
                emit AmountAdjusted(_sender, _token, amount, balance);
                balances[_sender][_token] -= balance;
                return -int256(balance);
            } else {
                balances[_sender][_token] -= amount;
                return _amount;
            }
        } else {
            balances[_sender][_token] += uint256(_amount);
            return _amount;
        }
    }
}
