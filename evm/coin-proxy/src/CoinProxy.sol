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
    uint64 public lastDepositSequence;
    uint64 public lastWithdrawalSequence;

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

    function submitDeposits(bytes calldata data) public onlySubmitter {
        BatchDeposit memory _batchDeposit = abi.decode(data, (BatchDeposit));
        require(_batchDeposit.deposits.length > 0, "Must be at least 1 deposit");
        uint64 _sequence = _batchDeposit.deposits[0].sequence;
        require(_sequence >= lastDepositSequence, "Sequence must be increasing");
        for (uint256 i = 0; i < _batchDeposit.deposits.length; i++) {
            if (i > 0) {
                require(_batchDeposit.deposits[i].sequence > _sequence, "Sequence must be increasing");
            }
            balances[_batchDeposit.deposits[i].sender][_batchDeposit.deposits[i].token] +=
                _batchDeposit.deposits[i].amount;
            emit DepositSucceeded(
                _batchDeposit.deposits[i].sender,
                _batchDeposit.deposits[i].sequence,
                _batchDeposit.deposits[i].token,
                _batchDeposit.deposits[i].amount
            );
        }
        lastDepositSequence = _batchDeposit.deposits[_batchDeposit.deposits.length - 1].sequence;
    }

    function submitWithdrawalBatch(bytes calldata data) public onlySubmitter {
        require(batchHash == 0, "Settlement batch in process");
        BatchWithdrawal memory _batchWithdrawal = abi.decode(data, (BatchWithdrawal));
        require(_batchWithdrawal.withdrawals.length > 0, "Must be at least 1 withdrawal");
        uint64 _sequence = _batchWithdrawal.withdrawals[0].sequence;
        require(_sequence > lastWithdrawalSequence, "Sequence must be increasing");
        for (uint256 i = 0; i < _batchWithdrawal.withdrawals.length; i++) {
            if (i > 0) {
                require(_batchWithdrawal.withdrawals[i].sequence > _sequence, "Sequence must be increasing");
            }
            _withdraw(
                _batchWithdrawal.withdrawals[i].sequence,
                _batchWithdrawal.withdrawals[i].sender,
                _batchWithdrawal.withdrawals[i].token,
                _batchWithdrawal.withdrawals[i].amount,
                _batchWithdrawal.withdrawals[i].feeAmount
            );
        }
        lastDepositSequence = _batchWithdrawal.withdrawals[_batchWithdrawal.withdrawals.length - 1].sequence;
    }

    function rollbackWithdrawalBatch(bytes calldata data) public onlySubmitter {
        BatchWithdrawal memory _batchWithdrawal = abi.decode(data, (BatchWithdrawal));
        for (uint256 i = 0; i < _batchWithdrawal.withdrawals.length; i++) {
            _rollbackWithdraw(
                _batchWithdrawal.withdrawals[i].sequence,
                _batchWithdrawal.withdrawals[i].sender,
                _batchWithdrawal.withdrawals[i].token,
                _batchWithdrawal.withdrawals[i].amount,
                _batchWithdrawal.withdrawals[i].feeAmount
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

    function _calculateWithdrawalBatchHash(bytes[] calldata withdrawals) internal pure returns (bytes32) {
        bytes memory buffer = new bytes(0);
        for (uint256 i = 0; i < withdrawals.length; i++) {
            bytes32 txHash = keccak256(withdrawals[i]);
            buffer = bytes.concat(buffer, txHash);
        }
        return keccak256(buffer);
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
