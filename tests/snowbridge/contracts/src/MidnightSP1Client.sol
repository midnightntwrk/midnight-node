// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pragma solidity 0.8.34;

import {ISP1Verifier} from "sp1-contracts/ISP1Verifier.sol";
import {MMRProof} from "snowbridge/utils/MMRProof.sol";

/// Immutable SP1 receiver for the PoC Ethereum-address/stake BEEFY guest.
contract MidnightSP1Client {
    ISP1Verifier public immutable verifier;
    bytes32 public immutable programVKey;
    bytes32 public immutable genesisHash;
    bytes32 public checkpointHash;
    bytes public checkpoint;
    bytes32 public latestMMRRoot;
    uint32 public latestBeefyBlock;

    error InvalidEncoding();
    error InvalidCheckpoint();
    error StaleUpdate();
    event RootAccepted(uint32 indexed blockNumber, bytes32 indexed root, bytes32 checkpointHash);

    constructor(address verifier_, bytes32 vkey_, bytes memory initialCheckpoint) {
        require(verifier_.code.length != 0 && vkey_ != bytes32(0), "invalid verifier configuration");
        _validateCheckpoint(initialCheckpoint);
        verifier = ISP1Verifier(verifier_);
        programVKey = vkey_;
        genesisHash = _word(initialCheckpoint, 0);
        latestBeefyBlock = uint32(_little(initialCheckpoint, 32, 4));
        checkpoint = initialCheckpoint;
        checkpointHash = _stateHash(initialCheckpoint);
    }

    function submitProof(bytes calldata proofBytes, bytes calldata publicValues) external {
        // SCALE Output: previous hash, checkpoint, commitment hash, MMR root, signature count.
        if (publicValues.length != 181 && publicValues.length != 225) revert InvalidEncoding();
        bytes memory state = publicValues[32:publicValues.length - 68];
        _validateCheckpoint(state);
        if (bytes32(publicValues[:32]) != checkpointHash || _word(state, 0) != genesisHash) revert InvalidCheckpoint();
        uint32 height = uint32(_little(state, 32, 4));
        if (height <= latestBeefyBlock) revert StaleUpdate();
        verifier.verifyProof(programVKey, publicValues, proofBytes);
        bytes32 root = bytes32(publicValues[publicValues.length - 36:publicValues.length - 4]);
        checkpoint = state;
        checkpointHash = _stateHash(state);
        latestBeefyBlock = height;
        latestMMRRoot = root;
        emit RootAccepted(height, root, checkpointHash);
    }

    function verifyMMRLeafProof(bytes32 leafHash, bytes32[] calldata proof, uint256 proofOrder)
        external view returns (bool)
    {
        return latestMMRRoot != bytes32(0) && MMRProof.verifyLeafProof(latestMMRRoot, leafHash, proof, proofOrder);
    }

    /// Authority roots include stake; the guest also binds their keys to the runtime MMR.
    function currentCommittee() external view returns (uint64 id, uint32 count, bytes32 root) {
        bytes memory state = checkpoint;
        return (uint64(_little(state, 36, 8)), uint32(_little(state, 44, 4)), _word(state, 48));
    }

    function nextCommittee() external view returns (bool announced, uint64 id, uint32 count, bytes32 root) {
        bytes memory state = checkpoint;
        if (state[80] == 0) return (false, 0, 0, bytes32(0));
        return (true, uint64(_little(state, 81, 8)), uint32(_little(state, 89, 4)), _word(state, 93));
    }

    function _validateCheckpoint(bytes memory state) private pure {
        if (state.length != 81 && state.length != 125) revert InvalidEncoding();
        uint8 tag = uint8(state[80]);
        if ((tag == 0 && state.length != 81) || (tag == 1 && state.length != 125) || tag > 1) revert InvalidEncoding();
        uint256 count = _little(state, 44, 4);
        if (count == 0 || count > 4096) revert InvalidEncoding();
        if (tag == 1) {
            uint256 nextCount = _little(state, 89, 4);
            if (nextCount == 0 || nextCount > 4096 || _little(state, 81, 8) != _little(state, 36, 8) + 1) revert InvalidEncoding();
        }
    }
    function _stateHash(bytes memory state) private pure returns (bytes32) {
        return keccak256(abi.encodePacked("midnight-vector-state-v1", state));
    }
    function _little(bytes memory data, uint256 offset, uint256 size) private pure returns (uint256 value) {
        for (uint256 i; i < size; ++i) value |= uint256(uint8(data[offset + i])) << (8 * i);
    }
    function _word(bytes memory data, uint256 offset) private pure returns (bytes32 value) {
        assembly { value := mload(add(add(data, 32), offset)) }
    }
}
