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
    bytes32 public lastSettlementBatchHash;
    bytes32 public lastWithdrawalBatchHash;

    mapping(address => SovereignWithdrawal) public sovereignWithdrawals;
    uint256 public sovereignWithdrawalDelay;

    mapping(address => address) public linkedSigners;

    string constant WITHDRAW_SIGNATURE = "Withdraw(address sender,address token,uint256 amount,uint64 nonce)";

    function initialize(address _submitter, address _feeAccount, uint256 _sovereignWithdrawalDelay)
        public
        initializer
    {
        __Ownable_init(msg.sender);
        __UUPSUpgradeable_init();
        __EIP712_init("funkybit", "0.1.0");
        submitter = _submitter;
        feeAccount = _feeAccount;
        sovereignWithdrawalDelay = _sovereignWithdrawalDelay;
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

    function setSovereignWithdrawalDelay(uint256 _sovereignWithdrawalDelay) external onlyOwner {
        require(_sovereignWithdrawalDelay >= 1 days, "Not a valid sovereign withdrawal delay");
        sovereignWithdrawalDelay = _sovereignWithdrawalDelay;
    }

    function linkSigner(address _linkedSigner, bytes32 _digest, bytes calldata _signature) external {
        address recovered = ECDSA.recover(_digest, _signature);
        if (recovered == address(0) || (recovered != _linkedSigner)) {
            emit LinkSignerFailed(msg.sender, _linkedSigner);
        } else {
            linkedSigners[msg.sender] = _linkedSigner;
            emit LinkedSigner(msg.sender, _linkedSigner);
        }
    }

    function removeLinkedSigner() external {
        delete linkedSigners[msg.sender];
        emit LinkedSigner(msg.sender, address(0));
    }

    function deposit(address _token, uint256 _amount) external {
        IERC20 erc20 = IERC20(_token);
        erc20.transferFrom(msg.sender, address(this), _amount);

        balances[msg.sender][_token] += _amount;
        emit Deposit(msg.sender, _token, _amount);
    }

    function sovereignWithdrawal(address _token, uint256 _amount) external {
        SovereignWithdrawal memory request = sovereignWithdrawals[msg.sender];

        if (request.timestamp == 0) {
            // no active withdrawal request, initiate new withdrawal
            _sovereignWithdrawal(_token, _amount);
        } else if (block.timestamp < request.timestamp + sovereignWithdrawalDelay) {
            revert("Withdrawal delay not met");
        } else if (request.token == _token && request.amount == _amount) {
            // complete withdrawal
            if (request.amount == 0) {
                uint256 balance = balances[msg.sender][_token];
                _withdraw(0, msg.sender, request.token, balance, 0);
            } else {
                _withdraw(0, msg.sender, request.token, request.amount, 0);
            }
            delete sovereignWithdrawals[msg.sender];
        } else {
            // token or amount mismatch, initiate new withdrawal
            _sovereignWithdrawal(_token, _amount);
        }
    }

    function _sovereignWithdrawal(address _token, uint256 _amount) internal {
        uint256 balance = balances[msg.sender][_token];
        require(_amount <= balance, "Insufficient balance");

        sovereignWithdrawals[msg.sender] =
            SovereignWithdrawal({token: _token, amount: _amount, timestamp: block.timestamp});
        emit WithdrawalRequested(msg.sender, _token, _amount);
    }

    function _isZeroSignature(bytes memory signature) internal pure returns (bool) {
        if (signature.length != 65) {
            return false;
        }
        for (uint256 i = 0; i < signature.length; i++) {
            if (signature[i] != 0) {
                return false;
            }
        }
        return true;
    }

    function submitWithdrawals(bytes[] calldata withdrawals) public onlySubmitter {
        require(batchHash == 0, "Settlement batch in process");
        for (uint256 i = 0; i < withdrawals.length; i++) {
            TransactionType txType = TransactionType(uint8(withdrawals[i][0]));
            WithdrawWithSignature memory signedTx = abi.decode(withdrawals[i][1:], (WithdrawWithSignature));
            bool withdrawAll = txType == TransactionType.WithdrawAll;
            bool pendingSovereignWithdrawal =
                _isZeroSignature(signedTx.signature) && _isPendingSovereignWithdrawal(signedTx, withdrawAll);
            if (pendingSovereignWithdrawal || _validateWithdrawalSignature(signedTx, txType)) {
                if (withdrawAll) {
                    _withdrawAll(
                        signedTx.sequence,
                        signedTx.tx.sender,
                        signedTx.tx.token,
                        signedTx.tx.amount,
                        signedTx.tx.feeAmount
                    );
                } else {
                    _withdraw(
                        signedTx.sequence,
                        signedTx.tx.sender,
                        signedTx.tx.token,
                        signedTx.tx.amount,
                        signedTx.tx.feeAmount
                    );
                }
                if (pendingSovereignWithdrawal) {
                    delete sovereignWithdrawals[signedTx.tx.sender];
                }
            }
        }
        lastWithdrawalBatchHash = _calculateWithdrawalBatchHash(withdrawals);
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
            emit IExchange.SettlementCompleted(
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

    function _validateWithdrawalSignature(WithdrawWithSignature memory signedTx, TransactionType txType)
        internal
        returns (bool)
    {
        bytes32 digest = _hashTypedDataV4(
            keccak256(
                abi.encode(
                    keccak256(bytes(WITHDRAW_SIGNATURE)),
                    signedTx.tx.sender,
                    signedTx.tx.token,
                    txType == TransactionType.WithdrawAll ? 0 : signedTx.tx.amount,
                    signedTx.tx.nonce
                )
            )
        );
        address recovered = ECDSA.recover(digest, signedTx.signature);
        if (
            recovered == address(0)
                || (recovered != signedTx.tx.sender && recovered != linkedSigners[signedTx.tx.sender])
        ) {
            emit WithdrawalFailed(
                signedTx.tx.sender,
                signedTx.sequence,
                signedTx.tx.token,
                signedTx.tx.amount,
                0,
                ErrorCode.InvalidSignature
            );
            return false;
        }
        return true;
    }

    function _isPendingSovereignWithdrawal(WithdrawWithSignature memory signedTx, bool withdrawAll)
        internal
        view
        returns (bool)
    {
        SovereignWithdrawal memory request = sovereignWithdrawals[signedTx.tx.sender];
        return (request.amount == signedTx.tx.amount || request.amount == 0 && withdrawAll)
            && request.token == signedTx.tx.token;
    }

    function _withdrawAll(uint64 _sequence, address _sender, address _token, uint256 _amount, uint256 _fee) internal {
        uint256 balance = balances[_sender][_token];
        if (_fee >= balance) {
            emit WithdrawalFailed(_sender, _sequence, _token, _amount, balance, ErrorCode.InsufficientBalance);
        } else {
            _withdraw(_sequence, _sender, _token, _amount > balance ? balance : _amount, _fee);
        }
    }

    function _withdraw(uint64 _sequence, address _sender, address _token, uint256 _amount, uint256 _fee) internal {
        uint256 balance = balances[_sender][_token];
        if (_amount > balance) {
            emit WithdrawalFailed(_sender, _sequence, _token, _amount, balance, ErrorCode.InsufficientBalance);
        } else {
            balances[_sender][_token] -= _amount;
            balances[feeAccount][_token] += _fee;
            if (_token == address(0)) {
                payable(_sender).transfer(_amount - _fee);
            } else {
                IERC20 erc20 = IERC20(_token);
                erc20.transfer(_sender, _amount - _fee);
            }

            emit Withdrawal(_sender, _sequence, _token, _amount, _fee);
        }
    }
}
