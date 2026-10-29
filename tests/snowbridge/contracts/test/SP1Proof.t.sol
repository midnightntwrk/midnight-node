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
import {Test} from "forge-std/Test.sol";
import {SP1Verifier} from "sp1-contracts/v6.1.0/SP1VerifierGroth16.sol";
import {MidnightSP1Client} from "../src/MidnightSP1Client.sol";

contract SP1ProofTest is Test {
    function testCapturedRealGroth16Proof() public {
        string memory path=vm.envOr("SNOWBRIDGE_SP1_PROOF",string(""));
        if(bytes(path).length==0) {vm.skip(true);return;}
        string memory fixture=vm.readFile(path);
        bytes memory initial=vm.readFileBinary(vm.envString("SNOWBRIDGE_SP1_INITIAL"));
        bytes memory pv=vm.parseJsonBytes(fixture,".publicValues");
        bytes memory proof=vm.parseJsonBytes(fixture,".proofBytes");
        bytes32 key=vm.parseJsonBytes32(fixture,".programVKey");
        SP1Verifier verifier=new SP1Verifier();
        MidnightSP1Client client=new MidnightSP1Client(address(verifier),key,initial);
        bytes32 old=client.checkpointHash();
        bytes memory bad=bytes.concat(proof); bad[bad.length-1]^=0x01;
        vm.expectRevert();client.submitProof(bad,pv);
        bad=bytes.concat(pv); bad[150]^=0x01;
        vm.expectRevert();client.submitProof(proof,bad);
        vm.expectRevert();verifier.verifyProof(bytes32(uint256(key)^1),pv,proof);
        bad=bytes.concat(pv);bad[0]^=0x01;
        vm.expectRevert(MidnightSP1Client.InvalidCheckpoint.selector);client.submitProof(proof,bad);
        bad=bytes.concat(pv);bad[32]^=0x01;
        vm.expectRevert(MidnightSP1Client.InvalidCheckpoint.selector);client.submitProof(proof,bad);
        assertEq(client.checkpointHash(),old);assertEq(client.latestMMRRoot(),bytes32(0));
        client.submitProof(proof,pv);
        assertEq(client.latestMMRRoot(),vm.parseJsonBytes32(fixture,".mmrRoot"));
        assertEq(client.latestBeefyBlock(),vm.parseJsonUint(fixture,".block"));
        assertEq(client.checkpointHash(),vm.parseJsonBytes32(fixture,".checkpointHash"));
        assertEq(client.checkpoint(),vm.parseJsonBytes(fixture,".checkpoint"));
        vm.expectRevert(MidnightSP1Client.InvalidCheckpoint.selector);client.submitProof(proof,pv);
    }
}
