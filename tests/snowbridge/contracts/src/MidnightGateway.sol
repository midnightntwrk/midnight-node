// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pragma solidity 0.8.34;

import {Gateway} from "snowbridge/Gateway.sol";
import {BeefyClient} from "snowbridge/BeefyClient.sol";
import {Verification} from "snowbridge/Verification.sol";
import {Initializer} from "snowbridge/Initializer.sol";
import {ScaleCodec} from "snowbridge/utils/ScaleCodec.sol";
import {CallsV2 as Calls} from "snowbridge/v2/Calls.sol";
import {HandlersV2 as Handlers} from "snowbridge/v2/Handlers.sol";
import {Network, Command, CommandKind} from "snowbridge/v2/Types.sol";

/// Disposable, single-asset deployment. The upstream patch disables V1 and requires one command.
contract MidnightGateway is Gateway {
    bytes32 private constant CONFIG_SLOT = keccak256("midnight.snowbridge.poc.v1");
    uint128 public constant MAX_AMOUNT = 1_000_000_000_000;
    uint64 public constant MAX_DEPOSITS = 64;

    struct Route {
        bytes32 domain;
        bytes32 origin;
        address token;
        uint64 deposits;
        bool registered;
    }

    struct Config {
        Initializer.Config gateway;
        bytes32 ethereumGenesis;
        bytes32 midnightGenesis;
        bytes32 salt;
        address token;
    }

    error InvalidRoute();
    error InvalidEnvelope();
    error UnsupportedOperation();
    error CapacityExhausted();

    constructor(address client, address executor) Gateway(client, executor) {}

    function route() private pure returns (Route storage r) {
        bytes32 slot = CONFIG_SLOT;
        assembly { r.slot := slot }
    }

    function bridgeDomain() external view returns (bytes32) {
        return route().domain;
    }

    function bridgeOrigin() external view returns (bytes32) {
        return route().origin;
    }

    function depositCount() external view returns (uint64) {
        return route().deposits;
    }

    function initialize(bytes calldata data) external override {
        Config memory config = abi.decode(data, (Config));
        // The upstream initializer checks proxy context and creates its own custody Agent.
        Initializer.initialize(abi.encode(config.gateway));
        require(config.token != address(0) && config.token.code.length != 0, InvalidRoute());
        Route storage r = route();
        require(r.token == address(0), InvalidRoute());
        r.domain = keccak256(
            abi.encode(
                bytes32("MN/SB/DOMAIN/V1"),
                block.chainid,
                config.ethereumGenesis,
                address(this),
                config.midnightGenesis,
                config.salt
            )
        );
        r.origin = keccak256(abi.encode(bytes32("MN/SB/ORIGIN/V1"), r.domain));
        r.token = config.token;
    }

    function v2_registerToken(address token, uint8 network, uint128 executionFee, uint128 relayerFee)
        external
        payable
        override
        nonreentrant
    {
        Route storage r = route();
        require(
            token == r.token && network == 0 && msg.value == 0 && executionFee == 0 && relayerFee == 0 && !r.registered,
            InvalidRoute()
        );
        r.registered = true;
        Calls.registerToken(token, Network(network), 0, 0);
    }

    function v2_sendMessage(
        bytes calldata envelope,
        bytes[] calldata assets,
        bytes calldata claimer,
        uint128 executionFee,
        uint128 relayerFee
    ) external payable override nonreentrant {
        Route storage r = route();
        require(
            envelope.length == 96 && assets.length == 1 && assets[0].length == 96 && claimer.length == 0
                && msg.value == 0 && executionFee == 0 && relayerFee == 0,
            InvalidEnvelope()
        );
        (uint8 version, bytes32 domain, bytes32 recipient) = abi.decode(envelope, (uint8, bytes32, bytes32));
        (uint8 kind, address token, uint128 amount) = abi.decode(assets[0], (uint8, address, uint128));
        require(version == 1 && domain == r.domain && recipient != bytes32(0), InvalidEnvelope());
        require(kind == 0 && token == r.token && amount > 0 && amount <= MAX_AMOUNT, InvalidRoute());
        require(r.deposits < MAX_DEPOSITS, CapacityExhausted());
        r.deposits++;
        Calls.sendMessage(envelope, assets, claimer, 0, 0);
    }

    function v2_dispatchCommand(Command calldata command, bytes32 origin) external override onlySelf {
        Route storage r = route();
        require(
            origin == r.origin && command.kind == CommandKind.UnlockNativeToken && command.payload.length == 96,
            UnsupportedOperation()
        );
        (address token, address recipient, uint128 amount) = abi.decode(command.payload, (address, address, uint128));
        require(token == r.token && recipient != address(0) && amount > 0 && amount <= MAX_AMOUNT, InvalidRoute());
        Handlers.unlockNativeToken(AGENT_EXECUTOR, command.payload);
    }

    function _verifyCommitment(bytes32 commitment, Verification.Proof calldata proof, bool isV2)
        internal
        view
        override
        returns (bool)
    {
        if (
            !isV2 || proof.header.parentHash != 0 || proof.header.number != 0 || proof.header.stateRoot != 0
                || proof.header.extrinsicsRoot != 0 || proof.header.digestItems.length != 0 || proof.headProof.pos != 0
                || proof.headProof.width != 0 || proof.headProof.proof.length != 0
        ) return false;
        Verification.MMRLeafPartial calldata leaf = proof.leafPartial;
        // Version 1.0 identifies the standalone fixed-width application-root layout.
        if (leaf.version != 32) return false;
        bytes32 leafHash = keccak256(
            bytes.concat(
                ScaleCodec.encodeU8(leaf.version),
                ScaleCodec.encodeU32(leaf.parentNumber),
                leaf.parentHash,
                ScaleCodec.encodeU64(leaf.nextAuthoritySetID),
                ScaleCodec.encodeU32(leaf.nextAuthoritySetLen),
                leaf.nextAuthoritySetRoot,
                commitment
            )
        );
        return BeefyClient(BEEFY_CLIENT).verifyMMRLeafProof(leafHash, proof.leafProof, proof.leafProofOrder);
    }
}
