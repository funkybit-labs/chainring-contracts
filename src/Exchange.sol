// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "./common/Constants.sol";
import "./interfaces/IVersion.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {Initializable} from "openzeppelin-contracts/contracts/proxy/utils/Initializable.sol";
import {OwnableUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/access/OwnableUpgradeable.sol";
import {UUPSUpgradeable} from "openzeppelin-contracts-upgradeable/contracts/proxy/utils/UUPSUpgradeable.sol";

contract Exchange is UUPSUpgradeable, OwnableUpgradeable, IVersion {
    mapping(address => mapping(address => uint256)) public balances;
    mapping(address => uint256) public nativeBalances;

    event DepositCreated();
    event WithdrawalCreated(uint256);

    error ErrorInsufficientBalance(uint256);

    function initialize() public initializer {
        __Ownable_init(msg.sender);
        __UUPSUpgradeable_init();
    }

    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}

    function getVersion() external pure returns (uint64) {
        return VERSION;
    }

    function deposit(address _token, uint256 _amount) external {
        IERC20 erc20 = IERC20(_token);
        erc20.transferFrom(msg.sender, address(this), _amount);

        balances[msg.sender][_token] += _amount;
        emit DepositCreated();
    }

    receive() external payable {
        nativeBalances[msg.sender] += msg.value;
        emit DepositCreated();
    }

    function withdraw(address _token, uint256 _amount) external {
        uint256 balance = balances[msg.sender][_token];
        if (_amount != 0) {
            if (balance < _amount) {
                revert ErrorInsufficientBalance(balance);
            }
        } else {
            _amount = balance;
        }

        IERC20 erc20 = IERC20(_token);
        erc20.transfer(msg.sender, _amount);

        balances[msg.sender][_token] -= _amount;
        emit WithdrawalCreated(_amount);
    }

    function withdraw(uint256 _amount) external {
        uint256 balance = nativeBalances[msg.sender];
        if (_amount != 0) {
            if (balance < _amount) {
                revert ErrorInsufficientBalance(balance);
            }
        } else {
            _amount = balance;
        }
        payable(msg.sender).transfer(_amount);

        nativeBalances[msg.sender] -= _amount;
        emit WithdrawalCreated(_amount);
    }
}
