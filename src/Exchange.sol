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
import "forge-std/console.sol";

contract Exchange is EIP712Upgradeable, UUPSUpgradeable, OwnableUpgradeable, IExchange {
    mapping(address => mapping(address => uint256)) public balances;
    uint256 public txProcessedCount;
    address public submitter;
    address public feeAccount;
    mapping(address => uint8) public tokenPrecision;
    bytes32 public batchHash;

    string constant WITHDRAW_SIGNATURE = "Withdraw(address sender,address token,uint256 amount,uint64 nonce)";
    string constant WITHDRAW_NATIVE_SIGNATURE = "Withdraw(address sender,uint256 amount,uint64 nonce)";
    string constant ORDER_SIGNATURE =
        "Order(address sender,address baseToken,address quoteToken,int256 amount,uint256 price,int256 nonce)";

    struct TokenBalance {
        address sender;
        address token;
        uint256 balance;
    }

    struct SavedBalances {
        uint32 count;
        TokenBalance[] balances;
    }

    function initialize(address _submitter, address _feeAccount, uint8 _nativePrecision) public initializer {
        __Ownable_init(msg.sender);
        __UUPSUpgradeable_init();
        __EIP712_init("ChainRing Labs", "0.0.1");
        submitter = _submitter;
        feeAccount = _feeAccount;
        tokenPrecision[address(0)] = _nativePrecision;
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

    function submitBatch(bytes[] calldata transactions) public onlySubmitter {
        require(batchHash != 0, "No batch prepared");
        require(batchHash == _calculateBatchHash(transactions), "Hash does not match prepared batch");
        for (uint256 i = 0; i < transactions.length; i++) {
            bytes calldata transaction = transactions[i];
            processTransaction(transaction);
        }
        txProcessedCount += transactions.length;
        batchHash = 0;
    }

    function processTransaction(bytes calldata transaction) internal {
        TransactionType txType = TransactionType(uint8(transaction[0]));
        if (txType == TransactionType.Withdraw) {
            WithdrawWithSignature memory signedTx = abi.decode(transaction[1:], (WithdrawWithSignature));
            _withdraw(signedTx.tx.sender, signedTx.tx.token, signedTx.tx.amount);
        } else if (txType == TransactionType.WithdrawNative) {
            WithdrawNativeWithSignature memory signedTx = abi.decode(transaction[1:], (WithdrawNativeWithSignature));
            _withdraw(signedTx.tx.sender, address(0), signedTx.tx.amount);
        } else if (txType == TransactionType.SettleTrade) {
            _settleTrade(abi.decode(transaction[1:], (SettleTrade)));
        }
    }

    function prepareBatch(bytes[] calldata transactions) public onlySubmitter {
        require(batchHash == 0, "Batch in progress, submit or rollback");
        bool batchSucceeded = true;

        // A single settlement changes 4 balances
        SavedBalances memory savedBalances = SavedBalances(0, new TokenBalance[](transactions.length * 4));
        for (uint256 i = 0; i < transactions.length; i++) {
            bytes calldata transaction = transactions[i];
            if (!prepareTransaction(transaction, savedBalances)) {
                batchSucceeded = false;
            }
        }
        // balances are restored in reverse order so we go back to original values
        for (uint32 i = savedBalances.count; i > 0; i--) {
            uint32 index = i - 1;
            balances[savedBalances.balances[index].sender][savedBalances.balances[index].token] =
                savedBalances.balances[index].balance;
        }

        if (batchSucceeded) {
            batchHash = _calculateBatchHash(transactions);
        }
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

    function prepareTransaction(bytes calldata transaction, SavedBalances memory savedBalances)
        internal
        returns (bool)
    {
        TransactionType txType = TransactionType(uint8(transaction[0]));
        if (txType == TransactionType.Withdraw) {
            WithdrawWithSignature memory signedTx = abi.decode(transaction[1:], (WithdrawWithSignature));
            _saveBalance(signedTx.tx.sender, signedTx.tx.token, savedBalances);
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
            if (!_validateSignature(signedTx.tx.sender, digest, signedTx.signature, signedTx.sequence)) {
                return false;
            }
            return _withdrawDryRun(signedTx.tx.sender, signedTx.tx.token, signedTx.tx.amount, signedTx.sequence);
        } else if (txType == TransactionType.WithdrawNative) {
            WithdrawNativeWithSignature memory signedTx = abi.decode(transaction[1:], (WithdrawNativeWithSignature));
            _saveBalance(signedTx.tx.sender, address(0), savedBalances);
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
            if (!_validateSignature(signedTx.tx.sender, digest, signedTx.signature, signedTx.sequence)) {
                return false;
            }
            return _withdrawDryRun(signedTx.tx.sender, address(0), signedTx.tx.amount, signedTx.sequence);
        } else if (txType == TransactionType.SettleTrade) {
            return _settleTradeDryRun(abi.decode(transaction[1:], (SettleTrade)), savedBalances);
        } else {
            require(false, "Unknown transaction");
            return false;
        }
    }

    function _saveBalance(address _sender, address _token, SavedBalances memory savedBalances) internal view {
        savedBalances.balances[savedBalances.count] = TokenBalance(_sender, _token, balances[_sender][_token]);
        savedBalances.count += 1;
    }

    function _validateSignature(address _sender, bytes32 _digest, bytes memory _signature, uint64 sequence)
        internal
        returns (bool)
    {
        address recovered = ECDSA.recover(_digest, _signature);
        if (recovered != _sender) {
            emit PrepareTransactionFailed(sequence, ErrorCode.InvalidSignature);
            return false;
        }
        return true;
    }

    function _validateOrder(address _baseToken, address _quoteToken, OrderWithSignature memory _order)
        internal
        virtual
        returns (bytes32)
    {
        bytes32 digest = _hashTypedDataV4(
            keccak256(
                abi.encode(
                    keccak256(bytes(ORDER_SIGNATURE)),
                    _order.tx.sender,
                    _baseToken,
                    _quoteToken,
                    _order.tx.amount,
                    _order.tx.price,
                    _order.tx.nonce
                )
            )
        );

        return digest;
    }

    function _withdrawDryRun(address _sender, address _token, uint256 _amount, uint64 sequence)
        internal
        returns (bool)
    {
        uint256 balance = balances[_sender][_token];
        if (_amount == 0) {
            _amount = balance;
        }
        return _adjustBalanceDryRun(_sender, _token, -int256(_amount), sequence, true);
    }

    function _withdraw(address _sender, address _token, uint256 _amount) internal {
        uint256 balance = balances[_sender][_token];
        if (_amount == 0) {
            _amount = balance;
        }
        uint256 _actual = uint256(-_adjustBalance(_sender, _token, -int256(_amount), true));

        if (_token == address(0)) {
            payable(_sender).transfer(_actual);
        } else {
            IERC20 erc20 = IERC20(_token);
            erc20.transfer(_sender, _actual);
        }

        emit Withdrawal(_sender, _token, _actual);
    }

    function _settleTradeDryRun(SettleTrade memory _trade, SavedBalances memory savedBalances)
        internal
        returns (bool)
    {
        //
        // trade.amount is positive if taker is buying and negative if selling
        // fee amounts are passed in (can be 0) and are taken in the quote currency from taker and maker.
        //
        int256 baseAdjustment = _trade.amount;
        int256 notional = (_trade.amount * int256(_trade.price)) / int256(10 ** _tokenPrecision(_trade.baseToken));
        int256 takerQuoteAdjustment = -notional - int256(_trade.takerFee);
        int256 makerQuoteAdjustment = notional - int256(_trade.makerFee);

        _saveBalance(_trade.takerOrder.tx.sender, _trade.baseToken, savedBalances);
        _saveBalance(_trade.makerOrder.tx.sender, _trade.baseToken, savedBalances);
        _saveBalance(_trade.takerOrder.tx.sender, _trade.quoteToken, savedBalances);
        _saveBalance(_trade.makerOrder.tx.sender, _trade.quoteToken, savedBalances);

        return _adjustBalanceDryRun(
            _trade.takerOrder.tx.sender, _trade.baseToken, baseAdjustment, _trade.sequence, false
        )
            && _adjustBalanceDryRun(_trade.makerOrder.tx.sender, _trade.baseToken, -baseAdjustment, _trade.sequence, false)
            && _adjustBalanceDryRun(
                _trade.takerOrder.tx.sender, _trade.quoteToken, takerQuoteAdjustment, _trade.sequence, false
            )
            && _adjustBalanceDryRun(
                _trade.makerOrder.tx.sender, _trade.quoteToken, makerQuoteAdjustment, _trade.sequence, false
            );
    }

    function _adjustBalanceDryRun(
        address _sender,
        address _token,
        int256 _amount,
        uint64 sequence,
        bool _allowAboveBalance
    ) internal returns (bool) {
        if (_amount < 0) {
            uint256 balance = balances[_sender][_token];
            uint256 amount = uint256(-_amount);
            if (amount > balance) {
                if (_allowAboveBalance) {
                    emit AmountAdjusted(_sender, _token, amount, balance);
                    balances[_sender][_token] -= balance;
                    return true;
                } else {
                    emit PrepareTransactionFailed(sequence, ErrorCode.InsufficientBalance);
                    return false;
                }
            } else {
                balances[_sender][_token] -= amount;
                return true;
            }
        } else {
            balances[_sender][_token] += uint256(_amount);
            return true;
        }
    }

    function _adjustBalance(address _sender, address _token, int256 _amount, bool _allowAboveBalance)
        internal
        returns (int256)
    {
        if (_amount < 0) {
            uint256 balance = balances[_sender][_token];
            uint256 amount = uint256(-_amount);
            if (amount > balance) {
                if (_allowAboveBalance) {
                    emit AmountAdjusted(_sender, _token, amount, balance);
                    balances[_sender][_token] -= balance;
                    return -int256(balance);
                } else {
                    revert("Insufficient balance");
                }
            } else {
                balances[_sender][_token] -= amount;
                return _amount;
            }
        } else {
            balances[_sender][_token] += uint256(_amount);
            return _amount;
        }
    }

    function _settleTrade(SettleTrade memory _trade) internal {
        // verify the original orders - for now just generates the digest, signatures are not checked
        bytes32 takerOrderDigest = _validateOrder(_trade.baseToken, _trade.quoteToken, _trade.takerOrder);
        bytes32 makerOrderDigest = _validateOrder(_trade.baseToken, _trade.quoteToken, _trade.makerOrder);

        //
        // trade.amount is positive if taker is buying and negative if selling
        // fee amounts are passed in (can be 0) and are taken in the quote currency from taker and maker.
        //
        int256 baseAdjustment = _trade.amount;
        int256 notional = (_trade.amount * int256(_trade.price)) / int256(10 ** _tokenPrecision(_trade.baseToken));
        _adjustBalance(_trade.takerOrder.tx.sender, _trade.baseToken, baseAdjustment, false);
        _adjustBalance(_trade.makerOrder.tx.sender, _trade.baseToken, -baseAdjustment, false);

        int256 takerQuoteAdjustment = -notional - int256(_trade.takerFee);
        int256 makerQuoteAdjustment = notional - int256(_trade.makerFee);
        _adjustBalance(_trade.takerOrder.tx.sender, _trade.quoteToken, takerQuoteAdjustment, false);
        _adjustBalance(_trade.makerOrder.tx.sender, _trade.quoteToken, makerQuoteAdjustment, false);

        // fees go to a fee account
        if (_trade.takerFee + _trade.makerFee > 0) {
            _adjustBalance(feeAccount, _trade.quoteToken, int256(_trade.takerFee + _trade.makerFee), false);
        }

        // we emit an order filled event for both the taker and maker orders. This includes the original order and
        // fill amounts for the trade and balance adjustments. If there were multiple partial fills then the
        // orderDigest would be the same for each fill for a given order
        //
        emit OrderFilled(
            takerOrderDigest,
            _trade.takerOrder.tx.sender,
            _trade.baseToken,
            _trade.quoteToken,
            true,
            _trade.takerOrder.tx,
            ExecutionInfo({
                filledAmount: baseAdjustment,
                executionPrice: _trade.price,
                fee: _trade.takerFee,
                baseAdjustment: baseAdjustment,
                quoteAdjustment: takerQuoteAdjustment
            })
        );

        emit OrderFilled(
            makerOrderDigest,
            _trade.makerOrder.tx.sender,
            _trade.baseToken,
            _trade.quoteToken,
            false,
            _trade.makerOrder.tx,
            ExecutionInfo({
                filledAmount: -baseAdjustment,
                executionPrice: _trade.price,
                fee: _trade.makerFee,
                baseAdjustment: -baseAdjustment,
                quoteAdjustment: makerQuoteAdjustment
            })
        );
    }

    function _tokenPrecision(address _token) internal returns (uint8) {
        uint8 precision = tokenPrecision[_token];
        if (precision == 0) {
            precision = ERC20(_token).decimals();
            tokenPrecision[_token] = precision;
        }
        return precision;
    }
}
