// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import {CoinProxy} from "../../src/CoinProxy.sol";

contract CoinProxyUpgrade is CoinProxy {
    uint256 public value;

    function setValue(uint256 _value) public onlyOwner {
        value = _value;
    }
}
