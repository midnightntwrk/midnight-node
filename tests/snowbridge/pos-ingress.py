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

"""Validate native token bridging with real Ethereum PoS and Midnight consensus proofs."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / "artifacts/snowbridge"
spec = importlib.util.spec_from_file_location("pos_stack", ROOT / "tests/snowbridge/pos-stack.py")
stack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stack)


def execute(args, log, env, cwd=ROOT):
    print("Running:", " ".join(str(a) for a in args), flush=True)
    with log.open("w") as output:
        subprocess.run([str(a) for a in args], cwd=cwd, env=env, stdout=output,
            stderr=subprocess.STDOUT, check=True, timeout=1800)


def prepare_ingress(run, env):
    config = json.loads((run / "network.json").read_text())
    deadline = time.monotonic() + 1200
    while True:
        try:
            header = stack.request(config["beacon"] + "/eth/v1/beacon/headers/finalized")["data"]
            slot = int(header["header"]["message"]["slot"])
            if slot >= 32:
                break
            print(f"Waiting for Ethereum finality; finalized slot {slot}", flush=True)
        except (OSError, RuntimeError):
            pass
        if time.monotonic() >= deadline:
            raise TimeoutError("Ethereum did not finalize")
        time.sleep(20)
    proof = ART / "tools/beacon-proof"
    endpoints = ["--beacon", config["beacon"], "--execution", config["execution"]]
    if not (run / "bootstrap/checkpoint.scale").is_file():
        execute([proof, "--mode", "checkpoint", *endpoints, "--out", run / "bootstrap"], run / "checkpoint.log", env)
    nonce=stack.request(config["execution"], "eth_getTransactionCount", ["0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266", "pending"])
    assert int(nonce,16) == 0, "Refusing to redeploy into a chain with prior development transactions"
    env["SNOWBRIDGE_POS_DEPLOYMENT"] = str(run / "deployment.json")
    execute([ART / "tools/foundry/forge", "script", "script/PosDeposit.s.sol:PosDeposit",
        "--no-dynamic-test-linking", "--rpc-url", config["execution"], "--broadcast", "--slow",
        "--private-key", "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        "--priority-gas-price", "1000000000", "--with-gas-price", "2000000000"], run / "deploy.log", env, ROOT / "tests/snowbridge/contracts")
    broadcast = json.loads((ROOT / "tests/snowbridge/contracts/broadcast/PosDeposit.s.sol/31337/run-latest.json").read_text())
    (run / "broadcast.json").write_text(json.dumps(broadcast, indent=2) + "\n")
    deployment = json.loads((run / "deployment.json").read_text())
    transaction = broadcast["transactions"][-1]["hash"]
    receipt = stack.request(config["execution"], "eth_getTransactionReceipt", [transaction])
    assert int(receipt["status"], 16) == 1
    event, = [log for log in receipt["logs"] if log["address"].lower() == deployment["gateway"].lower()]
    custody_word = deployment["custody"].removeprefix("0x").lower().rjust(64, "0")
    balance = stack.request(config["execution"], "eth_call", [{"to": deployment["token"], "data": "0x70a08231" + custody_word}, receipt["blockNumber"]])
    assert int(balance, 16) == 100
    execute([proof, "--mode", "receipt", *endpoints, "--tx", transaction,
        "--log-index", str(int(event["logIndex"], 16)), "--out", run / "receipt"], run / "proof.log", env)
    env["SNOWBRIDGE_POS_PROOFS"] = str(run)
    execute(["cargo", "test", "--locked", "-p", "pallet-ethereum-bridge", "live_pos_receipt_is_authenticated",
        "--", "--ignored", "--nocapture"], run / "receipt-test.log", env)

if __name__ == "__main__":
    prepare_ingress(Path(os.environ["SNOWBRIDGE_PROOF_DIR"]), dict(os.environ))
