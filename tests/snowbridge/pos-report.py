#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Validate and hash evidence from completed live PoS/native-ingress stages."""
import hashlib
import json
from pathlib import Path
import sys
import urllib.request

ROOT=Path(__file__).resolve().parents[2]
ART=ROOT / "artifacts/snowbridge"
run=Path(sys.argv[1]).resolve()
assert run.is_relative_to(ART / "ethereum-pos")
read=lambda name:json.loads((run / name).read_text())
live=read("midnight-beacon-evidence.json")
assert "failure" not in live
assert live["bootstrapFinalized"] and live["liveBeaconUpdateVerified"]
assert live["nativeTokenIssuedLive"] and live["duplicateIssuanceRejected"] and live["duplicateReceiptRejected"]
assert live["ledgerBefore"]["contractCount"] == 0
assert live["ledgerAfter"]["contractCount"] == 1
assert live["ledgerAfter"]["unshieldedUtxoCount"] == live["ledgerBefore"]["unshieldedUtxoCount"] + 1
assert live["ledgerAfter"]["unshieldedUtxoStars"] == live["ledgerBefore"]["unshieldedUtxoStars"]
assert live["ledgerAfter"]["dustNullifierCount"] > live["ledgerBefore"]["dustNullifierCount"]
for name in ["receipt-test.log","ledger-build.log","issuance-replay.log"]:
    log=(run / name).read_text()
    assert "test result: ok. 1 passed" in log and "test result: FAILED" not in log, name
proof=read("receipt/evidence.json")
status=read("status.json")
assert int(status["finalizedExecution"]["number"],16) >= proof["executionBlock"]
network=read("network.json")
deployment=read("deployment.json")
def rpc(method,params):
    request=urllib.request.Request(network["execution"],json.dumps({"jsonrpc":"2.0","id":1,"method":method,"params":params}).encode(),{"content-type":"application/json"})
    with urllib.request.urlopen(request,timeout=10) as response:
        result=json.load(response)
    assert "error" not in result,result
    return result["result"]
block=rpc("eth_getBlockByNumber",[hex(proof["executionBlock"]),False])
assert block["hash"] == proof["block"]
word=deployment["custody"].removeprefix("0x").lower().rjust(64,"0")
custody=int(rpc("eth_call",[{"to":deployment["token"],"data":"0x70a08231"+word},status["finalizedExecution"]["number"]]),16)
outbound=live.get("outboundRedemptionLive",False)
assert custody == (60 if outbound else 100)
smoke=read("midnight-node-smoke.json")
assert smoke["genesisHash"] == live["genesisHash"] and smoke["finalizedNumber"] >= 3
files=["bootstrap/checkpoint.scale","receipt/update.scale","receipt/event.scale","receipt/evidence.json",
    "receipt-test.log","midnight-beacon-evidence.json","midnight-node-smoke.json","network.json","compose.json","status.json","broadcast.json",
    "ledger/registry.json","ledger/genesis-state.bin","ledger/escrow-proven.bin","ledger/live-issuance.bin","issuance-replay.log"]
compact=live.get("liveCompactConsumerRoundTrip",False)
if compact:
    assert live["reissuanceAfterSpendingRejected"]
    assert [step["consumerBalance"] for step in live["compactSteps"]] == ["0","100","100","60","60"]
    assert "test result: ok. 1 passed" in (run / "compact-prove.log").read_text()
    assert live["compact"]["consumerBalance"] == 60 and live["compact"]["escrowBalance"] == 40
    assert live["compact"]["walletBalance"] == 0
    assert live["compact"]["consumerBalance"] + live["compact"]["walletBalance"] + (0 if outbound else live["compact"]["escrowBalance"]) == custody
    assert live["compact"]["ledgerStats"]["contractCount"] == 2
    assert live["compact"]["ledgerStats"]["unshieldedUtxoCount"] == live["ledgerBefore"]["unshieldedUtxoCount"]
    files += ["compact-prove.log","ledger/compact-manifest.json","ledger/live-consumer-state.bin","ledger/live-escrow-state.bin"]
    files += [f"ledger/compact-proven-{index}.bin" for index in range(5)]
if live.get("verifierRepair"):
    assert live["verifierRepair"]["persistedFinalizedHistoryRechecked"]
    files += ["midnight-beacon-evidence-before-verifier-fix.json", "midnight-readback.log", "execution-finalized-readback.json"]
if outbound:
    assert compact and live["completeBridgeE2E"]
    result=live["outbound"]
    assert result["phase"] == "Completed" and result["liveConsensusProofs"]
    assert [attempt["success"] for attempt in result["attempts"]] == [False,True]
    assert result["attempts"][0]["nonce"] != result["attempts"][1]["nonce"]
    assert all(attempt["finalizedReceiptVerified"] for attempt in result["attempts"])
    recipient=int(rpc("eth_call",[{"to":deployment["token"],"data":"0x70a08231"+("05"*20).rjust(64,"0")},status["finalizedExecution"]["number"]]),16)
    assert recipient == 40 and custody + recipient == 100
    checkpoint=read("beefy-checkpoint.json")
    assert checkpoint["midnightGenesis"] == smoke["genesisHash"]
    assert checkpoint["midnightGenesisLedgerRoot"] == smoke["genesisLedgerRoot"]
    assert checkpoint["ethereumGenesis"] == rpc("eth_getBlockByNumber",["0x0",False])["hash"]
    files += ["beefy-checkpoint.json","deployment.json","genesis-build.log"]
    for index in range(2):
        release=read(f"release-receipt-{index}/evidence.json")
        assert int(status["finalizedExecution"]["number"],16) >= release["executionBlock"]
        assert rpc("eth_getBlockByNumber",[hex(release["executionBlock"]),False])["hash"] == release["block"]
        attempt=result["attempts"][index]
        assert release["transaction"] == attempt["ethereumTransaction"]
        receipt=rpc("eth_getTransactionReceipt",[attempt["ethereumTransaction"]])
        assert int(receipt["status"],16) == 1 and receipt["blockHash"] == release["block"]
        # keccak256(InboundMessageDispatched(uint64,bytes32,bool,bytes32)).
        dispatch_topic="0x8856ab63954e6c2938803a4654fb704c8779757e7bfdbe94a578e341ec637a95"
        log,=[entry for entry in receipt["logs"] if entry["address"].lower() == deployment["gateway"].lower() and entry["topics"][0] == dispatch_topic]
        assert len(log["topics"]) == 2 and int(log["topics"][1],16) == attempt["nonce"]
        data=bytes.fromhex(log["data"].removeprefix("0x"))
        assert len(data) == 96 and "0x"+data[:32].hex() == result["id"]
        assert int.from_bytes(data[32:64],"big") == index
        assert int(log["logIndex"],16) == release["logIndex"]
        files += [f"release-receipt-{index}/{name}" for name in ["event.scale","update.scale","evidence.json"]]
        files += [f"outbound-proof-{index}.json",f"mmr-{index}.json",f"mmr-input-{index}.json",f"mmr-simple-{index}/simplified.json",f"release-broadcast-{index}.json"]
    files += [p.name for p in run.glob("beefy-*.scale")]
sp1=deployment.get("sp1",False)
if sp1:
    assert outbound
    key=read("sp1-key.json")
    assert key["sdkVersion"] == "6.10.0" and key["circuitVersion"] == "v6.1.0"
    elf=ART/"sp1-target/elf-compilation/riscv64im-succinct-zkvm-elf/release/midnight-sp1-guest"
    elf_bytes=elf.read_bytes()
    assert hashlib.sha256(elf_bytes).hexdigest() == key["elfSha256"]
    (run/"sp1-guest.elf").write_bytes(elf_bytes)
    files += ["sp1-guest.elf"]
    initial=(run/"sp1-initial.scale").read_bytes()
    assert "0x"+initial[:32].hex() == smoke["genesisHash"]
    assert "0x"+initial.hex() == checkpoint["sp1Checkpoint"]
    previous=rpc("web3_sha3",["0x"+(b"midnight-vector-state-v1"+initial).hex()])
    verifier_code_hash=rpc("web3_sha3",[rpc("eth_getCode",[deployment["sp1Verifier"],status["finalizedExecution"]["number"]])])
    receiver_code_hash=rpc("web3_sha3",[rpc("eth_getCode",[deployment["beefyClient"],status["finalizedExecution"]["number"]])])
    for attempt in result["attempts"]:
        accepted=attempt["sp1"]
        assert accepted["realGroth16"] and accepted["finalized"]
        job=read(f"sp1/job-{accepted['block']}.json")
        assert job["phase"] == "Finalized" and job["negativeChecks"] == 6
        assert job["proof"]["programVKey"] == key["programVKey"]
        assert job["proof"]["elfSha256"] == key["elfSha256"]
        assert job["proof"]["profile"] == "ethereum-address-stake-v2-committee-mmr"
        assert job["proof"]["committeeStateVerified"] is True
        for block in job["proof"]["verifiedBlocks"]:
            committee=read(f"committee-{block}.json")
            assert committee["block"] == block
            assert int(committee["leafIndex"]) + 1 == int(committee["leafCount"])
            leaf=bytes.fromhex(committee["leaf"].removeprefix("0x"))
            assert len(leaf) == 113 and leaf[0] == 0x20
            assert int.from_bytes(leaf[1:5], "little") + 1 == block
        assert job["proof"]["mmrRoot"] == accepted["mmrRoot"]
        assert job["proof"]["checkpointHash"] == accepted["checkpointHash"]
        assert job["proof"]["previousStateHash"] == previous
        assert job["verifierCodeHash"] == verifier_code_hash
        assert job["receiverCodeHash"] == receiver_code_hash
        previous=accepted["checkpointHash"]
        receipt=rpc("eth_getTransactionReceipt",[accepted["transaction"]])
        assert int(receipt["status"],16) == 1
        assert receipt["to"].lower() == deployment["beefyClient"].lower()
        assert int(status["finalizedExecution"]["number"],16) >= int(receipt["blockNumber"],16)
        assert rpc("eth_getBlockByNumber",[receipt["blockNumber"],False])["hash"] == receipt["blockHash"]
        assert receipt["blockHash"] == job["receipt"]["blockHash"]
        assert int(receipt["gasUsed"],16) == int(accepted["gasUsed"])
        root_topic="0xf433844eba9a8b98d2cb5c2e3a103178f84043e9d27fffb0faf04171d1aa0c49"
        root_log,=[entry for entry in receipt["logs"] if entry["address"].lower() == deployment["beefyClient"].lower() and entry["topics"][0] == root_topic]
        assert len(root_log["topics"]) == 3
        assert int(root_log["topics"][1],16) == accepted["block"]
        assert root_log["topics"][2] == accepted["mmrRoot"]
        assert root_log["data"] == accepted["checkpointHash"]
        release=read(f"outbound-proof-{attempt['index']}.json")
        assert release["commitment"]["blockNumber"] == accepted["block"]
        assert next(p["data"] for p in release["commitment"]["payload"] if p["payloadID"] == "0x6d68") == accepted["mmrRoot"]
    files += ["sp1-key.json", "sp1-initial.scale", "sp1-observed.json"]
    files += [p.name for p in sorted(run.glob("committee-*.json"))]
    if (run/"sp1-backfill.json").exists(): files += ["sp1-backfill.json"]
    files += [str(p.relative_to(run)) for p in sorted((run/"sp1").rglob("*")) if p.is_file()]
manual_files = sorted(run.glob("manual/*.request.json")) + sorted(run.glob("manual/*.response.json"))
files += [str(path.relative_to(run)) for path in manual_files]
evidence={"profile":"live-pos-bidirectional-compact" if outbound else ("live-pos-compact-ingress" if compact else "live-pos-native-ingress"),"completeBridgeE2E":outbound,
    "operatorPaced":bool(manual_files), "sp1FinalityProved":sp1, "committeeStateVerified":sp1,
    "nativeTokenIssuedLive":True,"outboundRedemptionLive":outbound,"liveCompactConsumerRoundTrip":compact,
    "ethereumCustody":custody,"nativeAmountIssued":100,"realPoSReceiptVerifiedByPallet":True,"liveMidnightBeaconUpdate":True,
    "escrowAndLedgerDoublesInPalletTest":True,"liveNodeUsesRealEscrowAndLedger":True,
    "outboundCheckpointSyntheticAndUnused":not outbound,"deploymentChainBindingHashesAreFixtures":not outbound,
    "ledgerFees":"default, funded with real DUST proofs",
    "localStackRecovery":network.get("recovery"),
    "compactResult":live.get("compact"),
    "outboundResult":live.get("outbound"),
    "verifierRepair":live.get("verifierRepair"),
    "binarySha256":smoke["binarySha256"],"metadataSha256":smoke["metadataSha256"],
    "sha256":{name:hashlib.sha256((run/name).read_bytes()).hexdigest() for name in files}}
(run / "validation.json").write_text(json.dumps(evidence,indent=2)+"\n")
print("PASS: live PoS receipt, Midnight beacon update, native issuance" + (", Compact round trip" if compact else "") + (", failed release, retry and completed Ethereum redemption" if outbound else "") + f". Evidence: {run}/validation.json")
