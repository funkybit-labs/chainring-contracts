// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";

contract Exchange {
    mapping(address => mapping(address => uint256)) public balances;
    address public owner = msg.sender;

    event DepositCreated();
    event WithdrawalCreated(uint256);

    function deposit(address _token, uint256 _amount) external {
        IERC20 erc20 = IERC20(_token);
        erc20.transferFrom(msg.sender, address(this), _amount);

        balances[msg.sender][_token] += _amount;
        emit DepositCreated();
    }

    function withdraw(address _token, uint256 _amount) external {
        uint256 balance = balances[msg.sender][_token];
        if (_amount != 0) {
            require(balance >= _amount);
        } else {
            _amount = balance;
        }

        IERC20 erc20 = IERC20(_token);
        erc20.transfer(msg.sender, _amount);

        balances[msg.sender][_token] -= _amount;
        emit WithdrawalCreated(_amount);
    }
}
