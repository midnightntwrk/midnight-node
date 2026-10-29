#!/usr/bin/env python3
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Read-only observer for the local execution fixture; does not authenticate consensus."""
import argparse
import json
import hashlib
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--rpc", default="http://127.0.0.1:18545")
args = parser.parse_args()

def rpc(method, params):
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    request = urllib.request.Request(args.rpc, payload, {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=10) as response:
        result = json.load(response)
    if "error" in result:
        raise RuntimeError(result["error"])
    return result["result"]

manifest = json.loads((ROOT / "artifacts/snowbridge/deployment.json").read_text())
broadcast = json.loads((ROOT / "tests/snowbridge/contracts/broadcast/Local.s.sol/31337/run-latest.json").read_text())
assert int(rpc("eth_chainId", []), 16) == 31337
block = rpc("eth_getBlockByNumber", ["latest", False])
block_number = block["number"]

def call(address, data):
    return rpc("eth_call", [{"to": address, "data": data}, block_number])

def balance(owner):
    return int(call(manifest["token"], "0x70a08231" + owner[2:].lower().zfill(64)), 16)

assert balance(manifest["custody"]) == 60
assert balance(manifest["recipient"]) == 40
receipts = []
for tx in broadcast["transactions"]:
    receipt = rpc("eth_getTransactionReceipt", [tx["hash"]])
    assert receipt is not None and int(receipt["status"], 16) == 1, tx["hash"]
    receipts.append(receipt)

# Derive event selectors using the deployed ABI's signatures through web3_sha3.
def selector(signature):
    return rpc("web3_sha3", ["0x" + signature.encode().hex()])

dispatch_topic = selector("InboundMessageDispatched(uint64,bytes32,bool,bytes32)")
deposit_topic = selector("OutboundMessageAccepted(uint64,(address,(uint8,bytes)[],(uint8,bytes),bytes,uint128,uint128,uint128))")
gateway_logs = [log for r in receipts for log in r["logs"] if log["address"].lower() == manifest["gateway"].lower()]
dispatches = [log for log in gateway_logs if log["topics"][0] == dispatch_topic]
assert len(dispatches) == 2
assert [int(log["topics"][1], 16) for log in dispatches] == [1, 2]
assert [int(log["data"][66:130], 16) for log in dispatches] == [1, 0]
deposits = [log for log in gateway_logs if log["topics"][0] == deposit_topic]
assert len(deposits) == 2, "registration and deposit must be separate logs"
transactions = []
deployed_code = []
for tx, receipt in zip(broadcast["transactions"], receipts):
    calldata = rpc("eth_getTransactionByHash", [tx["hash"]])["input"]
    transactions.append({"hash": tx["hash"], "contract": tx.get("contractName"),
        "function": tx.get("function"), "gasUsed": int(receipt["gasUsed"], 16),
        "calldataBytes": (len(calldata) - 2) // 2, "input": calldata})
    if receipt.get("contractAddress"):
        code = rpc("eth_getCode", [receipt["contractAddress"], block_number])
        size = (len(code) - 2) // 2
        assert 0 < size <= 24576
        assert (len(calldata) - 2) // 2 <= 49152
        deployed_code.append({"name": tx.get("contractName"), "address": receipt["contractAddress"],
            "runtimeBytes": size, "runtimeSha256": hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()})
    if tx.get("function", "") and tx["function"].startswith("submitFiatShamir"):
        assert int(receipt["gasUsed"], 16) <= 5_000_000
    if any(log["topics"][0] == dispatch_topic for log in receipt["logs"]):
        assert int(receipt["gasUsed"], 16) <= 5_000_000
for name in ["beefyClient", "gateway", "token"]:
    code = rpc("eth_getCode", [manifest[name], block_number])
    assert 0 < (len(code) - 2) // 2 <= 24576
report = {
    "profile": "ethereum-execution-only-synthetic-beefy", "liveMidnightConsensus": False,
    "ethereumBeaconFinalityVerified": False, "block": block_number, "blockHash": block["hash"],
    "deployment": manifest, "custodyBalance": 60, "recipientBalance": 40,
    "dispatchSuccess": [True, False], "receipts": receipts,
    "transactions": transactions, "deployedCode": deployed_code,
    "evm": {"chainId": 31337, "hardfork": "prague", "blockGasLimit": 30_000_000,
        "proofTransactionGasCeiling": 5_000_000, "realCodeSizeLimits": True},
}
output = ROOT / "artifacts/snowbridge/live-evidence.json"
output.write_text(json.dumps(report, indent=2) + "\n")
(ROOT / "artifacts/snowbridge/deposit-logs.json").write_text(json.dumps(deposits, indent=2) + "\n")
print(json.dumps({"evidence": str(output), "custody": 60, "recipient": 40, "dispatchSuccess": [True, False]}))
