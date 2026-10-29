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
import {ISP1Verifier} from "sp1-contracts/ISP1Verifier.sol";
import {SP1Verifier} from "sp1-contracts/v6.1.0/SP1VerifierGroth16.sol";
import {MidnightSP1Client} from "../src/MidnightSP1Client.sol";

// Decoder/state tests only. Real proof acceptance is checked separately against SP1Verifier.
contract BoundVerifier is ISP1Verifier {
    bytes32 public expected;
    function bind(bytes32 key, bytes memory values, bytes memory proof) external {
        expected = keccak256(abi.encode(key, values, proof));
    }
    function verifyProof(bytes32 key, bytes calldata values, bytes calldata proof) external view {
        require(expected == keccak256(abi.encode(key, values, proof)), "invalid proof");
    }
}
contract SP1ClientTest is Test {
    BoundVerifier verifier;
    MidnightSP1Client client;
    bytes32 constant KEY = bytes32(uint256(1));
    function state(uint8 height, bool next) internal pure returns(bytes memory) {
        return abi.encodePacked(bytes32("genesis"), bytes1(height), hex"000000", hex"0100000000000000", hex"01000000", bytes32("authority"),
            next ? abi.encodePacked(hex"01", hex"0200000000000000", hex"01000000", bytes32("next")) : bytes(hex"00"));
    }
    function values(bytes memory cp) internal view returns(bytes memory) {
        return abi.encodePacked(client.checkpointHash(), cp, bytes32("commitment"), bytes32("root"), hex"01000000");
    }
    function setUp() public {
        verifier = new BoundVerifier();
        client = new MidnightSP1Client(address(verifier), KEY, state(0, true));
    }
    function testBothCanonicalOutputFormsAndMmr() public {
        for(uint8 i=1;i<=2;i++) {
            bytes memory cp=state(i,i==1); bytes memory pv=values(cp);
            assertEq(pv.length,i==1?225:181);
            verifier.bind(KEY,pv,hex"1234"); client.submitProof(hex"1234",pv);
            assertEq(client.checkpointHash(),keccak256(abi.encodePacked("midnight-vector-state-v1",cp)));
            assertEq(client.latestBeefyBlock(),i);
            assertTrue(client.verifyMMRLeafProof(bytes32("root"),new bytes32[](0),0));
            assertFalse(client.verifyMMRLeafProof(bytes32("other"),new bytes32[](0),0));
        }
    }
    function testCommittedCommitteeViewsAdvanceOnlyWithVerifiedProof() public {
        (uint64 id, uint32 count, bytes32 root) = client.currentCommittee();
        assertEq(id, 1); assertEq(count, 1); assertEq(root, bytes32("authority"));
        (bool announced, uint64 nextId, uint32 nextCount, bytes32 nextRoot) = client.nextCommittee();
        assertTrue(announced); assertEq(nextId, 2); assertEq(nextCount, 1); assertEq(nextRoot, bytes32("next"));
        bytes memory cp = state(1, true);
        cp[36] = 0x02; cp[81] = 0x03;
        for (uint256 i; i < 32; ++i) { cp[48+i] = nextRoot[i]; cp[93+i] = bytes32("successor")[i]; }
        bytes memory pv = values(cp);
        vm.expectRevert(); client.submitProof(hex"1234", pv);
        (id,,root) = client.currentCommittee();
        assertEq(id, 1); assertEq(root, bytes32("authority"));
        verifier.bind(KEY,pv,hex"1234"); client.submitProof(hex"1234",pv);
        (id,,root) = client.currentCommittee();
        assertEq(id, 2); assertEq(root, nextRoot);
        (announced,nextId,,nextRoot) = client.nextCommittee();
        assertTrue(announced); assertEq(nextId, 3); assertEq(nextRoot, bytes32("successor"));
    }
    function testMalformedOptionAndTrailingBytes() public {
        bytes memory cp=state(1,true); cp[80]=0x02;
        bytes memory pv=values(cp); vm.expectRevert(MidnightSP1Client.InvalidEncoding.selector); client.submitProof("",pv);
        cp[80]=0; pv=values(cp); vm.expectRevert(MidnightSP1Client.InvalidEncoding.selector); client.submitProof("",pv);
        pv=bytes.concat(values(state(1,true)),hex"00"); vm.expectRevert(MidnightSP1Client.InvalidEncoding.selector); client.submitProof("",pv);
    }
    function testWrongGenesisCheckpointStaleAndProofLeaveStateUnchanged() public {
        bytes32 initial=client.checkpointHash(); bytes memory pv=values(state(1,true));
        verifier.bind(KEY,pv,hex"1234");
        vm.expectRevert(); client.submitProof(hex"1235",pv);
        bytes memory bad=values(state(1,true)); bad[0]^=0x01;
        vm.expectRevert(MidnightSP1Client.InvalidCheckpoint.selector); client.submitProof(hex"1234",bad);
        bad=values(state(1,true)); bad[32]^=0x01;
        vm.expectRevert(MidnightSP1Client.InvalidCheckpoint.selector); client.submitProof(hex"1234",bad);
        bad=values(state(0,true)); vm.expectRevert(MidnightSP1Client.StaleUpdate.selector); client.submitProof(hex"1234",bad);
        assertEq(client.checkpointHash(),initial); assertEq(client.latestMMRRoot(),bytes32(0));
        client.submitProof(hex"1234",pv);
        vm.expectRevert(MidnightSP1Client.InvalidCheckpoint.selector); client.submitProof(hex"1234",pv);
    }
    function testRealVerifierRejectsDummyProof() public {
        MidnightSP1Client real=new MidnightSP1Client(address(new SP1Verifier()),KEY,state(0,true));
        bytes memory pv=values(state(1,true)); vm.expectRevert(); real.submitProof(hex"1234",pv);
        assertEq(real.latestBeefyBlock(),0);
    }
}
