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

"""Owned, fresh mainnet-preset Ethereum PoS network for receipt-proof validation.
Only public development keys are funded. No engine mock or trusted header injection.
The printed run directory identifies the exact compose project for status/down.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / "artifacts/snowbridge"
GETH = "ethereum/client-go@sha256:66eee4d024e0f8ef180f1fa9578cfaccb5926d543f315f6d7d609a71aa644873"
LODESTAR = "chainsafe/lodestar@sha256:5ea988a51fd31f072b4482c182e82c4e4da243bb183dc22893878db490b5e0bd"


def request(url, method=None, params=None):
    data = None if method is None else json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as response:
        result = json.load(response)
    if "error" in result:
        raise RuntimeError(result["error"])
    return result if method is None else result["result"]


def compose(run, *args):
    subprocess.run(["docker", "compose", "-f", str(run / "compose.json"), *args], check=True)


def status(run):
    config = json.loads((run / "network.json").read_text())
    eth, beacon = config["execution"], config["beacon"]
    genesis = request(beacon + "/eth/v1/beacon/genesis")["data"]
    spec = request(beacon + "/eth/v1/config/spec")["data"]
    assert int(spec["SLOT_DURATION_MS"]) == 4000
    assert int(spec["SLOTS_PER_EPOCH"]) == 32
    assert int(spec["SYNC_COMMITTEE_SIZE"]) == 512
    assert int(spec["FULU_FORK_EPOCH"]) == 100000000
    head = request(beacon + "/eth/v1/beacon/headers/head")["data"]
    final = request(beacon + "/eth/v1/beacon/headers/finalized")["data"]
    execution = request(eth, "eth_getBlockByNumber", ["finalized", False])
    assert execution is not None
    evidence = dict(config, beaconGenesis=genesis, head=head, finalized=final,
        finalizedExecution={key: execution[key] for key in ("number", "hash", "stateRoot", "receiptsRoot")},
        completeBridgeE2E=False, runtimeReceiptVerified=False)
    (run / "status.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence, indent=2), flush=True)
    return int(final["header"]["message"]["slot"])


def up():
    eth_port = int(os.environ.get("SNOWBRIDGE_POS_EL_PORT", "18545"))
    beacon_port = int(os.environ.get("SNOWBRIDGE_POS_CL_PORT", "19596"))
    for port in [eth_port, beacon_port]:
        with socket.socket() as probe:
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            probe.bind(("127.0.0.1", port))
    parent = ART / "ethereum-pos"
    parent.mkdir(exist_ok=True)
    run = Path(tempfile.mkdtemp(prefix="run-", dir=parent))
    name = "midnight-snowbridge-" + run.name
    genesis = json.loads((ART / "upstream/web/packages/test/config/genesis.json").read_text())
    genesis["config"]["chainId"] = 31337
    genesis["config"].pop("OsakaTime")
    genesis["config"]["blobSchedule"].pop("osaka")
    genesis["config"]["blobSchedule"]["prague"] = {"max": 9, "target": 6, "baseFeeUpdateFraction": 5007716}
    genesis["gasLimit"] = "30000000"
    genesis["alloc"] = {"f39fd6e51aad88f6f4ce6ab8827279cfffb92266": {"balance": "10000000000000000000000"}}
    (run / "genesis.json").write_text(json.dumps(genesis, indent=2) + "\n")
    (run / "jwtsecret").write_text("42" * 32 + "\n")
    volumes = [f"{run}:/poc"]
    config = {"name": name, "services": {
        "geth": {"image": GETH, "volumes": volumes,
            "ports": [f"127.0.0.1:{eth_port}:8545"],
            "command": ["--datadir=/poc/geth", "--networkid=31337", "--http", "--http.addr=0.0.0.0",
                "--http.vhosts=*", "--http.api=eth,net,web3,debug,txpool", "--authrpc.addr=0.0.0.0",
                "--authrpc.vhosts=*", "--authrpc.jwtsecret=/poc/jwtsecret", "--gcmode=archive",
                "--syncmode=full", "--state.scheme=hash", "--miner.gasprice=1", "--nodiscover", "--maxpeers=0"]},
    }}
    (run / "compose.json").write_text(json.dumps(config, indent=2) + "\n")
    print(f"Owned Ethereum PoS stack: {run}", flush=True)
    try:
        compose(run, "run", "--rm", "--no-deps", "geth", "--datadir=/poc/geth", "--state.scheme=hash", "init", "/poc/genesis.json")
        compose(run, "up", "-d", "geth")
        endpoint = f"http://127.0.0.1:{eth_port}"
        deadline = time.monotonic() + 60
        while True:
            try:
                block = request(endpoint, "eth_getBlockByNumber", ["0x0", False])
                break
            except (OSError, RuntimeError):
                if time.monotonic() > deadline:
                    raise TimeoutError("Geth did not start")
                time.sleep(1)
        command = ["dev", "--genesisValidators=8", f"--genesisTime={int(time.time()) + 30}",
            "--startValidators=0..7", "--rest.address=0.0.0.0", "--rest.port=9596", "--rest.namespace=*",
            "--eth1.providerUrls=http://geth:8545", "--execution.urls=http://geth:8551",
            "--execution.engineMock=false", "--dataDir=/poc/lodestar", "--genesisEth1Hash=" + block["hash"],
            "--jwt-secret=/poc/jwtsecret", "--chain.archiveStateEpochFrequency=1", "--serveHistoricalState=true",
            "--params.SECONDS_PER_SLOT=4", "--params.SLOT_DURATION_MS=4000", "--params.FULU_FORK_EPOCH=100000000"]
        command += ["--params." + fork + "_FORK_EPOCH=0" for fork in ["ALTAIR", "BELLATRIX", "CAPELLA", "DENEB", "ELECTRA"]]
        config["services"]["lodestar"] = {"image": LODESTAR, "volumes": volumes,
            "environment": {"LODESTAR_PRESET": "mainnet"}, "ports": [f"127.0.0.1:{beacon_port}:9596"], "command": command}
        (run / "compose.json").write_text(json.dumps(config, indent=2) + "\n")
        network = {"runDirectory": str(run), "project": name, "execution": endpoint,
            "beacon": f"http://127.0.0.1:{beacon_port}", "chainId": 31337, "executionGenesisHash": block["hash"],
            "gethImage": GETH, "lodestarImage": LODESTAR, "preset": "mainnet", "secondsPerSlot": 4,
            "slotsPerEpoch": 32, "syncCommitteeSize": 512, "fuluForkEpoch": 100000000}
        (run / "network.json").write_text(json.dumps(network, indent=2) + "\n")
        compose(run, "up", "-d", "lodestar")
        print(f"Started. Inspect with: python3 tests/snowbridge/pos-stack.py status {run}\nStop with: python3 tests/snowbridge/pos-stack.py down {run}", flush=True)
        return run
    except BaseException:
        compose(run, "down")
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["up", "status", "down"])
    parser.add_argument("run", nargs="?", type=Path)
    args = parser.parse_args()
    if args.action == "up":
        if args.run:
            parser.error("up always creates a fresh run")
        up()
    else:
        if not args.run or not (args.run / "network.json").is_file():
            parser.error("status/down require an owned run directory")
        run = args.run.resolve()
        assert run.is_relative_to(ART / "ethereum-pos")
        network = json.loads((run / "network.json").read_text())
        assert network["runDirectory"] == str(run)
        assert network["project"] == "midnight-snowbridge-" + run.name
        assert json.loads((run / "compose.json").read_text())["name"] == network["project"]
        if args.action == "status":
            status(run)
        else:
            compose(run, "down")


if __name__ == "__main__":
    main()
