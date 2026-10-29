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

"""Run and stop an owned release node; assert finalized blocks and PoC runtime metadata.
This is a node smoke test, not a bridged transfer or Ethereum finality test.
"""
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / "artifacts/snowbridge"
BINARY = Path(os.environ.get("SNOWBRIDGE_NODE", str(ROOT / "target/release/midnight-node")))
GENESIS = Path(os.environ.get("SNOWBRIDGE_GENESIS_DIR", str(ART / "local-node")))
PORT = int(os.environ.get("SNOWBRIDGE_NODE_PORT", "19944"))
P2P = int(os.environ.get("SNOWBRIDGE_P2P_PORT", "30334"))
for port in [PORT, P2P]:
    with socket.socket() as probe:
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        probe.bind(("127.0.0.1", port))
assert BINARY.is_file(), "Build the release node with ethereum-bridge-poc first"
assert (GENESIS / "genesis-state.bin").is_file(), "Generate the fresh PoC ledger genesis first"
existing = os.environ.get("SNOWBRIDGE_EXISTING_NODE_RUN")
if existing:
    run = Path(existing).resolve()
    assert run.is_relative_to(ART / "local-node") and (run / "chain-spec.json").is_file()
    assert os.environ.get("SNOWBRIDGE_VERIFY_ONLY") == "1", "Existing databases are supported only for readback"
else:
    run = Path(tempfile.mkdtemp(prefix="run-", dir=ART / "local-node"))
env = dict(os.environ, CFG_PRESET="dev", CHAINSPEC_ID=os.environ.get("SNOWBRIDGE_CHAIN_ID", "midnight_snowbridge-fixture"),
    CHAINSPEC_NAME="Disposable Snowbridge PoC",
    CHAINSPEC_GENESIS_STATE=str(GENESIS / "genesis-state.bin"),
    CHAINSPEC_GENESIS_BLOCK=str(GENESIS / "genesis-block.json"))

def rpc(method, params=None):
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []}).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}", data=payload, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as response:
        result = json.load(response)
    if "error" in result:
        raise RuntimeError(result["error"])
    return result["result"]

log_path = run / ("readback-node.log" if existing else "node.log")
log = log_path.open("w")
# --dev selects the embedded stock genesis. Build a custom spec from the configured
# PoC genesis first, then run its single public Alice authority.
if not existing:
    with (run / "chain-spec.json").open("w") as spec:
        subprocess.run([str(BINARY), "build-spec", "--disable-default-bootnode"],
            cwd=ROOT, env=env, stdout=spec, stderr=log, check=True)
process = subprocess.Popen([str(BINARY), "--chain", str(run / "chain-spec.json"), *(["--alice", "--force-authoring"] if not existing else []), "--base-path", str(run / "db"),
    "--node-key", "00" * 31 + "01", "--rpc-port", str(PORT), "--port", str(P2P), "--no-prometheus", "--no-telemetry", "--no-mdns",
    "--rpc-methods", "Unsafe", "--enable-offchain-indexing", "true", "--state-pruning", "archive", "--blocks-pruning", "archive"], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
print(f"Owned node PID {process.pid}; log {log_path}", flush=True)
try:
    deadline = time.monotonic() + 180
    while True:
        if process.poll() is not None:
            raise RuntimeError(f"Node exited {process.returncode}; see {log_path}")
        try:
            version = rpc("state_getRuntimeVersion")
            break
        except (OSError, RuntimeError):
            if time.monotonic() >= deadline:
                raise TimeoutError("Node RPC did not become available")
            time.sleep(1)
    metadata = rpc("state_getMetadata")
    raw = bytes.fromhex(metadata.removeprefix("0x"))
    assert b"EthereumBridge" in raw and b"EthereumBeaconClient" in raw, "Wrong runtime: bridge pallets missing"
    genesis = rpc("chain_getBlockHash", [0])
    initial_root = rpc("midnight_ledgerStateRoot", [genesis])
    while True:
        head = rpc("chain_getFinalizedHead")
        header = rpc("chain_getHeader", [head])
        number = int(header["number"], 16)
        if number >= 3:
            break
        if process.poll() is not None or time.monotonic() >= deadline:
            raise TimeoutError(f"Node did not finalize 3 blocks; latest {number}")
        time.sleep(2)
    driver = os.environ.get("SNOWBRIDGE_NODE_DRIVER")
    if driver:
        subprocess.run(["node", str(ROOT / "tests/snowbridge" / driver)], cwd=ROOT, env=env, check=True, timeout=None if env.get("SNOWBRIDGE_MANUAL") == "1" else (10800 if env.get("SNOWBRIDGE_SP1") == "true" else 3600))
    head = rpc("chain_getFinalizedHead")
    number = int(rpc("chain_getHeader", [head])["number"],16)
    root = rpc("midnight_ledgerStateRoot", [head])
    assert root and initial_root
    (run / "metadata.scale").write_bytes(raw)
    evidence = {"profile": "live-midnight-node-smoke", "completeBridgeE2E": False,
        "finalizedNumber": number, "finalizedHash": head, "genesisHash": genesis,
        "genesisLedgerRoot": initial_root, "finalizedLedgerRoot": root, "runtimeVersion": version,
        "metadataSha256": hashlib.sha256(raw).hexdigest(), "binarySha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
        "genesisSha256": hashlib.sha256((GENESIS / "genesis-state.bin").read_bytes()).hexdigest(),
        "runDirectory": str(run), "ethereumBeaconFinalityVerifiedLive": False, "bridgedTransfer": False}
    (ART / "node-smoke-evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(f"PASS: PoC runtime finalized {number} blocks; ledger root RPC works. Bridge driver results, if requested, are recorded separately.", flush=True)
finally:
    if process.poll() is None:
        process.send_signal(signal.SIGINT)
        try:
            process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)
    log.close()
