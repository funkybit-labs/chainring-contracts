// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import {IExchange} from "../../src/interfaces/IExchange.sol";

library SigUtils {
    // computes the hash of a permit
    function getStructHash(IExchange.Withdraw memory _withdraw) public pure returns (bytes32) {
        return keccak256(
            abi.encode(
                keccak256("Withdraw(address sender,address token,uint256 amount,uint64 nonce)"),
                _withdraw.sender,
                _withdraw.token,
                _withdraw.amount,
                _withdraw.nonce
            )
        );
    }

    function getStructHash(IExchange.WithdrawNative memory _withdraw) public pure returns (bytes32) {
        return keccak256(
            abi.encode(
                keccak256("Withdraw(address sender,uint256 amount,uint64 nonce)"),
                _withdraw.sender,
                _withdraw.amount,
                _withdraw.nonce
            )
        );
    }

    // computes the hash of the fully encoded EIP-712 message for the domain, which can be used to recover the signer
    function getTypedDataHash(bytes32 _domainSeparator, bytes32 _structHash) public pure returns (bytes32) {
        return keccak256(abi.encodePacked("\x19\x01", _domainSeparator, _structHash));
    }
}
