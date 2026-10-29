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
import {TestToken} from "../test/Bridge.t.sol";
import {Script} from "forge-std/Script.sol";
import {BeefyClient} from "snowbridge/BeefyClient.sol";
import {Verification} from "snowbridge/Verification.sol";
import {InboundMessage, Command, CommandKind} from "snowbridge/v2/Types.sol";
import {MidnightGateway} from "../src/MidnightGateway.sol";

import {MidnightSP1Client} from "../src/MidnightSP1Client.sol";

/// Relay observed consensus signatures and runtime proofs; no validator signing keys are used.
contract PosRelease is Script {
    function run() external {
        string memory deployment = vm.readFile(vm.envString("SNOWBRIDGE_POS_DEPLOYMENT"));
        string memory data = vm.readFile(vm.envString("SNOWBRIDGE_OUTBOUND_PROOF"));
        BeefyClient client = BeefyClient(vm.parseJsonAddress(deployment,".beefyClient"));
        MidnightGateway gateway = MidnightGateway(vm.parseJsonAddress(deployment,".gateway"));
        TestToken token = TestToken(vm.parseJsonAddress(deployment,".token"));
        uint256 count = vm.parseJsonUint(data,".payloadCount");
        BeefyClient.PayloadItem[] memory payload = new BeefyClient.PayloadItem[](count);
        for(uint256 i; i<count; i++) {
            string memory base = string.concat(".commitment.payload[",vm.toString(i),"]");
            payload[i] = BeefyClient.PayloadItem(bytes2(vm.parseJsonBytes(data,string.concat(base,".payloadID"))), vm.parseJsonBytes(data,string.concat(base,".data")));
        }
        BeefyClient.Commitment memory commitment = BeefyClient.Commitment(uint32(vm.parseJsonUint(data,".commitment.blockNumber")),uint64(vm.parseJsonUint(data,".commitment.validatorSetID")),payload);
        BeefyClient.ValidatorProof[] memory validators = new BeefyClient.ValidatorProof[](1);
        validators[0] = BeefyClient.ValidatorProof(uint8(vm.parseJsonUint(data,".validator.v")),vm.parseJsonBytes32(data,".validator.r"),vm.parseJsonBytes32(data,".validator.s"),0,vm.parseJsonAddress(data,".validator.account"),new bytes32[](0));
        uint256[] memory bitfield = new uint256[](1); bitfield[0]=1;
        BeefyClient.MMRLeaf memory leaf = BeefyClient.MMRLeaf(uint8(vm.parseJsonUint(data,".leaf.version")),uint32(vm.parseJsonUint(data,".leaf.parentNumber")),vm.parseJsonBytes32(data,".leaf.parentHash"),uint64(vm.parseJsonUint(data,".leaf.nextAuthoritySetID")),uint32(vm.parseJsonUint(data,".leaf.nextAuthoritySetLen")),vm.parseJsonBytes32(data,".leaf.nextAuthoritySetRoot"),vm.parseJsonBytes32(data,".leaf.parachainHeadsRoot"));
        bytes32[] memory path = vm.parseJsonBytes32Array(data,".leafProof");
        uint256 order = vm.parseJsonUint(data,".leafProofOrder");
        Command[] memory commands = new Command[](1);
        commands[0] = Command(uint8(vm.parseJsonUint(data,".message.commands[0].kind")),uint64(vm.parseJsonUint(data,".message.commands[0].gas")),vm.parseJsonBytes(data,".message.commands[0].payload"));
        InboundMessage memory message = InboundMessage(vm.parseJsonBytes32(data,".message.origin"),uint64(vm.parseJsonUint(data,".message.nonce")),vm.parseJsonBytes32(data,".message.topic"),commands);
        require(keccak256(abi.encode(message))==leaf.parachainHeadsRoot,"message root mismatch");
        Verification.Proof memory proof;
        proof.leafPartial = Verification.MMRLeafPartial(leaf.version,leaf.parentNumber,leaf.parentHash,leaf.nextAuthoritySetID,leaf.nextAuthoritySetLen,leaf.nextAuthoritySetRoot);
        proof.leafProof=path; proof.leafProofOrder=order;
        vm.startBroadcast();
        if (vm.envOr("SNOWBRIDGE_SP1", false)) {
            require(MidnightSP1Client(address(client)).latestBeefyBlock() == commitment.blockNumber, "SP1 block mismatch");
        } else {
            client.submitFiatShamir(commitment,bitfield,validators,leaf,path,order);
        }
        token.setFailRelease(vm.envBool("SNOWBRIDGE_FAIL_RELEASE"));
        gateway.v2_submit{gas: 1_000_000}(message,new bytes32[](0),proof,bytes32(0));
        vm.stopBroadcast();
        require(gateway.v2_isDispatched(message.nonce),"nonce not consumed");
    }
}
