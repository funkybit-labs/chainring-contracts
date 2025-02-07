// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "./common/Constants.sol";
import "./interfaces/IVersion.sol";
import "./interfaces/ICoinProxy.sol";
import {Initializable} from "openzeppelin-contracts/contracts/proxy/utils/Initializable.sol";
import {OwnableUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/access/OwnableUpgradeable.sol";
import {UUPSUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/proxy/utils/UUPSUpgradeable.sol";
import {EIP712Upgradeable} from "openzeppelin-contracts-upgradeable/contracts/utils/cryptography/EIP712Upgradeable.sol";

contract CoinProxy is EIP712Upgradeable, UUPSUpgradeable, OwnableUpgradeable, ICoinProxy {
    mapping(address => mapping(address => uint256)) public balances;
    address public submitter;
    address public feeAccount;
    bytes32 public batchHash;
    bytes32 public lastSettlementBatchHash;
    bytes32 public lastDepositAndWithdrawBatchHash;

    function initialize(address _submitter, address _feeAccount) public initializer {
        __Ownable_init(msg.sender);
        __UUPSUpgradeable_init();
        __EIP712_init("funkybit", "0.1.0");
        submitter = _submitter;
        feeAccount = _feeAccount;
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

    function submitDepositAndWithdrawalBatch(bytes calldata data) public onlySubmitter {
        require(batchHash == 0, "Settlement batch in progress");
        require(lastDepositAndWithdrawBatchHash != keccak256(data), "Matches last batch processed");
        BatchDepositAndWithdrawal memory _batch = abi.decode(data, (BatchDepositAndWithdrawal));
        require(_batch.withdrawals.length > 0 || _batch.deposits.length > 0, "Must be at least 1 deposit or withdrawal");
        for (uint256 i = 0; i < _batch.deposits.length; i++) {
            balances[_batch.deposits[i].sender][_batch.deposits[i].token] += _batch.deposits[i].amount;
            emit DepositSucceeded(
                _batch.deposits[i].sender,
                _batch.deposits[i].sequence,
                _batch.deposits[i].token,
                _batch.deposits[i].amount
            );
        }
        for (uint256 i = 0; i < _batch.withdrawals.length; i++) {
            _withdraw(
                _batch.withdrawals[i].sequence,
                _batch.withdrawals[i].sender,
                _batch.withdrawals[i].token,
                _batch.withdrawals[i].amount,
                _batch.withdrawals[i].feeAmount
            );
        }
        lastDepositAndWithdrawBatchHash = keccak256(data);
    }

    function rollbackWithdrawalBatch(bytes calldata data) public onlySubmitter {
        BatchWithdrawalRollback memory _batch = abi.decode(data, (BatchWithdrawalRollback));
        for (uint256 i = 0; i < _batch.withdrawals.length; i++) {
            _rollbackWithdraw(
                _batch.withdrawals[i].sequence,
                _batch.withdrawals[i].sender,
                _batch.withdrawals[i].token,
                _batch.withdrawals[i].amount,
                _batch.withdrawals[i].feeAmount
            );
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
                        _token,
                        _batchSettlement.walletTradeLists[walletIndex].tradeHashes,
                        _adjustmentAmount,
                        balances[_wallet][_token]
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
            emit ICoinProxy.SettlementCompleted(
                _batchSettlement.walletAddresses[i], _batchSettlement.walletTradeLists[i].tradeHashes
            );
        }

        lastSettlementBatchHash = batchHash;
        batchHash = 0;
    }

    function rollbackBatch() external onlySubmitter {
        batchHash = 0;
    }

    modifier onlySubmitter() {
        require(msg.sender == submitter, "Sender is not the submitter");
        _;
    }

    function _withdraw(uint64 _sequence, address _sender, address _token, uint256 _amount, uint256 _fee) internal {
        uint256 balance = balances[_sender][_token];
        if (_amount > balance) {
            emit WithdrawalFailed(_sender, _sequence, _token, _amount, balance, ErrorCode.InsufficientBalance);
        } else {
            if (_fee > 0 && _token != address(0)) {
                // for native the fee is included in amount, otherwise fee is separate
                uint256 fee_balance = balances[_sender][address(0)];
                if (_fee > fee_balance) {
                    emit WithdrawalFailed(
                        _sender, _sequence, _token, _fee, fee_balance, ErrorCode.InsufficientFeeBalance
                    );
                    return;
                }
                balances[_sender][address(0)] -= _fee;
            }
            balances[_sender][_token] -= _amount;
            balances[feeAccount][address(0)] += _fee;
            emit WithdrawalSucceeded(_sender, _sequence, _token, _amount, _fee);
        }
    }

    function _rollbackWithdraw(uint64 _sequence, address _sender, address _token, uint256 _amount, uint256 _fee)
        internal
    {
        balances[_sender][_token] += _amount;
        if (_fee > 0 && _token != address(0)) {
            balances[_sender][address(0)] += _fee;
        }
        balances[feeAccount][address(0)] -= _fee;
        emit WithdrawalRolledBack(_sender, _sequence, _token, _amount, _fee);
    }
}
