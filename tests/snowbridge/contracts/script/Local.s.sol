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
import {BridgeTest} from "../test/Bridge.t.sol";
import {BeefyClient} from "snowbridge/BeefyClient.sol";
import {InboundMessage} from "snowbridge/v2/Types.sol";

/// Broadcast-only execution fixture. Validator signatures are synthetic, never a live Midnight claim.
contract Local is BridgeTest {
    function setUp() public override {}

    function run() external {
        vm.startBroadcast();
        super.setUp();
        deposit(100);
        InboundMessage memory m = message(1, 40);
        BeefyClient.MMRLeaf memory leaf = leafFor(m);
        authenticate(leaf, 2, 1);
        gateway.v2_submit{gas: 5_000_000}(m, new bytes32[](0), proofFor(leaf), bytes32(0));
        token.setFailRelease(true);
        m = message(2, 10);
        leaf = leafFor(m);
        authenticate(leaf, 3, 1);
        gateway.v2_submit{gas: 5_000_000}(m, new bytes32[](0), proofFor(leaf), bytes32(0));
        vm.stopBroadcast();
        assertEq(token.balanceOf(custody), 60);
        assertEq(token.balanceOf(recipient), 40);
        string memory key = "deployment";
        vm.serializeAddress(key, "gateway", address(gateway));
        vm.serializeAddress(key, "beefyClient", address(client));
        vm.serializeAddress(key, "token", address(token));
        vm.serializeAddress(key, "custody", custody);
        string memory json = vm.serializeAddress(key, "recipient", recipient);
        vm.writeJson(json, "../../../artifacts/snowbridge/deployment.json");
    }
}
