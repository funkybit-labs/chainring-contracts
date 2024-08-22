// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import {Exchange} from "../../src/Exchange.sol";

contract ExchangeUpgrade is Exchange {
    uint256 public value;

    function setValue(uint256 _value) public onlyOwner {
        value = _value;
    }
}
