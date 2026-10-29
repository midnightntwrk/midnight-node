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

import {Test} from "forge-std/Test.sol";
import {BeefyClient} from "snowbridge/BeefyClient.sol";
import {GatewayProxy} from "snowbridge/GatewayProxy.sol";
import {AgentExecutor} from "snowbridge/AgentExecutor.sol";
import {Constants} from "snowbridge/Constants.sol";
import {Verification} from "snowbridge/Verification.sol";
import {Initializer} from "snowbridge/Initializer.sol";
import {OperatingMode} from "snowbridge/Types.sol";
import {InboundMessage, Command, CommandKind} from "snowbridge/v2/Types.sol";
import {IGatewayBase} from "snowbridge/interfaces/IGatewayBase.sol";
import {IGatewayV2} from "snowbridge/v2/IGateway.sol";
import {ERC20} from "openzeppelin/token/ERC20/ERC20.sol";
import {ScaleCodec} from "snowbridge/utils/ScaleCodec.sol";
import {MidnightGateway} from "../src/MidnightGateway.sol";

contract TestToken is ERC20 {
    bool public failRelease;

    constructor() ERC20("Local bridge token", "LBT") {
        _mint(msg.sender, 64e12);
    }

    function setFailRelease(bool fail) external {
        failRelease = fail;
    }

    function transfer(address recipient, uint256 amount) public override returns (bool) {
        require(!failRelease, "release disabled by test");
        return super.transfer(recipient, amount);
    }
}

/// Synthetic validator keys exercise the actual verifier; these are not live consensus proofs.
contract BridgeTest is Test {
    BeefyClient client;
    MidnightGateway gateway;
    TestToken token;
    bytes32[4] authorities;
    bytes32 authorityRoot;
    address custody;
    address recipient = address(0x1234);

    event InboundMessageDispatched(uint64 indexed nonce, bytes32 topic, bool success, bytes32 rewardAddress);

    function setUp() public virtual {
        for (uint256 i; i < 4; i++) {
            authorities[i] = keccak256(abi.encodePacked(vm.addr(i + 1)));
        }
        authorityRoot = keccak256(
            abi.encodePacked(
                keccak256(abi.encodePacked(authorities[0], authorities[1])),
                keccak256(abi.encodePacked(authorities[2], authorities[3]))
            )
        );
        client = new BeefyClient(
            0, 0, 1, 1, 0, BeefyClient.ValidatorSet(0, 4, authorityRoot), BeefyClient.ValidatorSet(1, 4, authorityRoot)
        );
        token = new TestToken();
        MidnightGateway implementation = new MidnightGateway(address(client), address(new AgentExecutor()));
        Initializer.Config memory config;
        config.mode = OperatingMode.Normal;
        gateway = MidnightGateway(
            address(
                new GatewayProxy(
                    address(implementation),
                    abi.encode(
                        MidnightGateway.Config(
                            config, bytes32(uint256(1)), bytes32(uint256(2)), bytes32(uint256(3)), address(token)
                        )
                    )
                )
            )
        );
        custody = gateway.agentOf(Constants.ASSET_HUB_AGENT_ID);
        gateway.v2_registerToken(address(token), 0, 0, 0);
        token.approve(address(gateway), type(uint256).max);
    }

    function deposit(uint128 amount) internal {
        bytes[] memory assets = new bytes[](1);
        assets[0] = abi.encode(uint8(0), address(token), amount);
        gateway.v2_sendMessage(abi.encode(uint8(1), gateway.bridgeDomain(), bytes32(uint256(7))), assets, "", 0, 0);
    }

    function message(uint64 nonce, uint128 amount) internal view returns (InboundMessage memory m) {
        Command[] memory commands = new Command[](1);
        commands[0] = Command(CommandKind.UnlockNativeToken, 200_000, abi.encode(address(token), recipient, amount));
        m = InboundMessage(gateway.bridgeOrigin(), nonce, keccak256("redemption-0"), commands);
    }

    function leafFor(InboundMessage memory m) internal view returns (BeefyClient.MMRLeaf memory leaf) {
        return BeefyClient.MMRLeaf(32, 1, keccak256("parent"), 2, 4, authorityRoot, keccak256(abi.encode(m)));
    }

    function leafHash(BeefyClient.MMRLeaf memory leaf) internal pure returns (bytes32) {
        return keccak256(
            bytes.concat(
                ScaleCodec.encodeU8(leaf.version),
                ScaleCodec.encodeU32(leaf.parentNumber),
                leaf.parentHash,
                ScaleCodec.encodeU64(leaf.nextAuthoritySetID),
                ScaleCodec.encodeU32(leaf.nextAuthoritySetLen),
                leaf.nextAuthoritySetRoot,
                leaf.parachainHeadsRoot
            )
        );
    }

    function commitment(BeefyClient.MMRLeaf memory leaf, uint32 blockNumber, uint64 set)
        internal
        pure
        returns (BeefyClient.Commitment memory c)
    {
        BeefyClient.PayloadItem[] memory payload = new BeefyClient.PayloadItem[](5);
        payload[0] = BeefyClient.PayloadItem("cb", hex"01020304");
        payload[1] = BeefyClient.PayloadItem("cs", hex"05060708");
        payload[2] = BeefyClient.PayloadItem("mh", abi.encodePacked(leafHash(leaf)));
        payload[3] = BeefyClient.PayloadItem("nb", hex"090a0b0c");
        payload[4] = BeefyClient.PayloadItem("ns", hex"0d0e0f10");
        return BeefyClient.Commitment(blockNumber, set, payload);
    }

    function signatures(BeefyClient.Commitment memory c)
        internal
        view
        returns (uint256[] memory bits, BeefyClient.ValidatorProof[] memory proofs)
    {
        bits = new uint256[](1);
        bits[0] = 7;
        proofs = new BeefyClient.ValidatorProof[](3);
        bytes32 hash = client.computeCommitmentHash(c);
        for (uint256 i; i < 3; i++) {
            bytes32[] memory path = new bytes32[](2);
            path[0] = authorities[i ^ 1];
            path[1] = i < 2
                ? keccak256(abi.encodePacked(authorities[2], authorities[3]))
                : keccak256(abi.encodePacked(authorities[0], authorities[1]));
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(i + 1, hash);
            proofs[i] = BeefyClient.ValidatorProof(v, r, s, i, vm.addr(i + 1), path);
        }
    }

    function authenticate(BeefyClient.MMRLeaf memory leaf, uint32 number, uint64 set) internal {
        BeefyClient.Commitment memory c = commitment(leaf, number, set);
        (uint256[] memory bits, BeefyClient.ValidatorProof[] memory proofs) = signatures(c);
        uint256 before = gasleft();
        client.submitFiatShamir(c, bits, proofs, leaf, new bytes32[](0), 0);
        assertLt(before - gasleft(), 5_000_000, "BEEFY gas ceiling");
        assertEq(client.latestMMRRoot(), leafHash(leaf));
    }

    function proofFor(BeefyClient.MMRLeaf memory leaf) internal pure returns (Verification.Proof memory p) {
        p.leafPartial = Verification.MMRLeafPartial(
            leaf.version,
            leaf.parentNumber,
            leaf.parentHash,
            leaf.nextAuthoritySetID,
            leaf.nextAuthoritySetLen,
            leaf.nextAuthoritySetRoot
        );
    }

    function testDepositProvedReleaseReplayAndRotation() public {
        deposit(100);
        assertEq(token.balanceOf(custody), 100);
        assertEq(gateway.depositCount(), 1);
        InboundMessage memory m = message(1, 40);
        BeefyClient.MMRLeaf memory leaf = leafFor(m);
        authenticate(leaf, 2, 1);
        (uint128 set,,,) = client.currentValidatorSet();
        assertEq(set, 1);
        vm.expectEmit(true, false, false, true, address(gateway));
        emit InboundMessageDispatched(1, m.topic, true, bytes32(0));
        uint256 before = gasleft();
        gateway.v2_submit(m, new bytes32[](0), proofFor(leaf), bytes32(0));
        assertLt(before - gasleft(), 5_000_000, "Gateway gas ceiling");
        assertEq(token.balanceOf(custody), 60);
        assertEq(token.balanceOf(recipient), 40);
        assertTrue(gateway.v2_isDispatched(1));
        vm.expectRevert(IGatewayBase.InvalidNonce.selector);
        gateway.v2_submit(m, new bytes32[](0), proofFor(leaf), bytes32(0));
    }

    function testReleaseFailureConsumesNonceAndEmitsFalse() public {
        deposit(100);
        InboundMessage memory m = message(1, 40);
        BeefyClient.MMRLeaf memory leaf = leafFor(m);
        authenticate(leaf, 2, 0);
        token.setFailRelease(true);
        vm.expectEmit(true, false, false, true, address(gateway));
        emit InboundMessageDispatched(1, m.topic, false, bytes32(0));
        gateway.v2_submit(m, new bytes32[](0), proofFor(leaf), bytes32(0));
        assertEq(token.balanceOf(custody), 100);
        assertEq(token.balanceOf(recipient), 0);
        assertTrue(gateway.v2_isDispatched(1));
        token.setFailRelease(false);
        vm.expectRevert(IGatewayBase.InvalidNonce.selector);
        gateway.v2_submit(m, new bytes32[](0), proofFor(leaf), bytes32(0));
    }

    function testChangedAmountAndNoncanonicalHeaderRejectWithoutConsumingNonce() public {
        deposit(100);
        InboundMessage memory m = message(1, 40);
        BeefyClient.MMRLeaf memory leaf = leafFor(m);
        authenticate(leaf, 2, 0);
        InboundMessage memory changed = message(1, 41);
        vm.expectRevert(IGatewayBase.InvalidProof.selector);
        gateway.v2_submit(changed, new bytes32[](0), proofFor(leaf), bytes32(0));
        Verification.Proof memory p = proofFor(leaf);
        p.header.number = 1;
        vm.expectRevert(IGatewayBase.InvalidProof.selector);
        gateway.v2_submit(m, new bytes32[](0), p, bytes32(0));
        assertFalse(gateway.v2_isDispatched(1));
        assertEq(token.balanceOf(custody), 100);
    }

    function testQuorumAndDuplicateSignatures() public {
        BeefyClient.MMRLeaf memory leaf = leafFor(message(1, 1));
        BeefyClient.Commitment memory c = commitment(leaf, 2, 0);
        (uint256[] memory bits, BeefyClient.ValidatorProof[] memory proofs) = signatures(c);
        uint256[] memory selected = client.createFiatShamirFinalBitfield(c, bits);
        assertEq(selected[0], 7, "must require three despite constructor minimum of one");
        BeefyClient.ValidatorProof[] memory two = new BeefyClient.ValidatorProof[](2);
        two[0] = proofs[0];
        two[1] = proofs[1];
        vm.expectRevert(BeefyClient.InvalidValidatorProofLength.selector);
        client.submitFiatShamir(c, bits, two, leaf, new bytes32[](0), 0);
        proofs[2] = proofs[0];
        vm.expectRevert(BeefyClient.InvalidValidatorProof.selector);
        client.submitFiatShamir(c, bits, proofs, leaf, new bytes32[](0), 0);
        assertEq(client.latestMMRRoot(), bytes32(0));
    }

    function testInteractivePathsDisabled() public {
        BeefyClient.MMRLeaf memory leaf = leafFor(message(1, 1));
        BeefyClient.Commitment memory c = commitment(leaf, 2, 0);
        (uint256[] memory bits, BeefyClient.ValidatorProof[] memory proofs) = signatures(c);
        vm.expectRevert(BeefyClient.InteractivePathDisabled.selector);
        client.submitInitial(c, bits, proofs[0]);
        vm.expectRevert(BeefyClient.InteractivePathDisabled.selector);
        client.commitPrevRandao(bytes32(0));
        vm.expectRevert(BeefyClient.InteractivePathDisabled.selector);
        client.submitFinal(c, bits, proofs, leaf, new bytes32[](0), 0);
    }

    function testMalformedDepositAndLifetimeCap() public {
        bytes[] memory assets = new bytes[](1);
        assets[0] = abi.encode(uint8(0), address(token), uint128(1));
        vm.expectRevert(MidnightGateway.InvalidEnvelope.selector);
        gateway.v2_sendMessage(abi.encode(uint8(1), bytes32(0), bytes32(uint256(7))), assets, "", 0, 0);
        assertEq(token.balanceOf(custody), 0);
        assertEq(gateway.depositCount(), 0);
        for (uint256 i; i < 64; i++) {
            deposit(1);
        }
        bytes memory envelope = abi.encode(uint8(1), gateway.bridgeDomain(), bytes32(uint256(7)));
        vm.expectRevert(MidnightGateway.CapacityExhausted.selector);
        gateway.v2_sendMessage(envelope, assets, "", 0, 0);
        assertEq(token.balanceOf(custody), 64);
    }

    function testSignaturePayloadPaddingAndStaleCommitmentReject() public {
        BeefyClient.MMRLeaf memory leaf = leafFor(message(1, 1));
        BeefyClient.Commitment memory c = commitment(leaf, 2, 0);
        (uint256[] memory bits, BeefyClient.ValidatorProof[] memory proofs) = signatures(c);
        c.payload[0].data = hex"ff";
        vm.expectRevert(BeefyClient.InvalidSignature.selector);
        client.submitFiatShamir(c, bits, proofs, leaf, new bytes32[](0), 0);
        c = commitment(leaf, 2, 0);
        bits[0] |= 1 << 255;
        vm.expectRevert();
        client.submitFiatShamir(c, bits, proofs, leaf, new bytes32[](0), 0);
        bits[0] = 7;
        client.submitFiatShamir(c, bits, proofs, leaf, new bytes32[](0), 0);
        vm.expectRevert(BeefyClient.StaleCommitment.selector);
        client.submitFiatShamir(c, bits, proofs, leaf, new bytes32[](0), 0);
    }

    function testQuorumSelectionForSmallAndChangingSetSizes() public {
        for (uint128 n = 1; n <= 10; n++) {
            BeefyClient sized = new BeefyClient(
                0,
                0,
                1,
                1,
                0,
                BeefyClient.ValidatorSet(0, n, authorityRoot),
                BeefyClient.ValidatorSet(1, n + 1, authorityRoot)
            );
            BeefyClient.Commitment memory c = commitment(leafFor(message(1, 1)), 2, 0);
            uint256[] memory bits = new uint256[](1);
            bits[0] = (uint256(1) << n) - 1;
            uint256[] memory selected = sized.createFiatShamirFinalBitfield(c, bits);
            uint256 count;
            for (uint256 i; i < n; i++) {
                count += (selected[0] >> i) & 1;
            }
            assertEq(count, n - (n - 1) / 3);
        }
    }

    function testUnsupportedIngressAndWrongOriginCannotRelease() public {
        vm.expectRevert(IGatewayV2.InvalidCommand.selector);
        gateway.depositEther();
        vm.expectRevert(IGatewayV2.InvalidCommand.selector);
        gateway.v2_createAgent(bytes32(uint256(88)));
        vm.expectRevert(MidnightGateway.InvalidRoute.selector);
        gateway.v2_registerToken(address(token), 0, 0, 0);
        deposit(100);
        InboundMessage memory m = message(1, 40);
        m.origin = bytes32(uint256(88));
        BeefyClient.MMRLeaf memory leaf = leafFor(m);
        authenticate(leaf, 2, 0);
        vm.expectEmit(true, false, false, true, address(gateway));
        emit InboundMessageDispatched(1, m.topic, false, bytes32(0));
        gateway.v2_submit(m, new bytes32[](0), proofFor(leaf), bytes32(0));
        assertEq(token.balanceOf(custody), 100);
        assertEq(token.balanceOf(recipient), 0);
        assertTrue(gateway.v2_isDispatched(1));
    }
}
