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
            stderr=subprocess.STDOUT, check=True, timeout=None if env.get("SNOWBRIDGE_MANUAL") == "1" else (10800 if env.get("SNOWBRIDGE_SP1") == "true" else 3600))


def main():
    subprocess.run(["bash", "tests/snowbridge/prepare-proof.sh"], cwd=ROOT, check=True)
    run = stack.up()
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target"))
    try:
        env.update(SNOWBRIDGE_PROOF_DIR=str(run), SNOWBRIDGE_RECEIPT_DIR=str(run / "receipt"),
            SNOWBRIDGE_NODE_DRIVER="node-live.mjs", SNOWBRIDGE_LIVE_ISSUANCE="1", SNOWBRIDGE_LIVE_OUTBOUND="1", SNOWBRIDGE_LIVE_COMPACT="1", SNOWBRIDGE_FEE_OUTPUTS="6", SNOWBRIDGE_CHAIN_ID="midnight_local-test",
            SNOWBRIDGE_GENESIS_DIR=str(run / "ledger"), SNOWBRIDGE_LIVE_LEDGER_DIR=str(run / "ledger"),
            MIDNIGHT_LEDGER_TEST_STATIC_DIR=str(ART / "compact"))
        (run / "ledger").mkdir()
        execute(["cargo", "test", "--locked", "--manifest-path", "tests/snowbridge/rust/Cargo.toml",
            "--features", "compact-ledger", "prepare_live_escrow_deployment", "--", "--ignored", "--nocapture"], run / "genesis-build.log", dict(env, SNOWBRIDGE_GENESIS_ONLY="1"))
        execute(["python3", "tests/snowbridge/node-smoke.py"], run / "midnight-live.log", env)
        execute(["cargo", "test", "--locked", "--manifest-path", "tests/snowbridge/rust/Cargo.toml",
            "--features", "compact-ledger", "replay_live_native_issuance_event", "--", "--ignored", "--nocapture"], run / "issuance-replay.log", env)
        stack.status(run)
        (run / "midnight-node-smoke.json").write_bytes((ART / "node-smoke-evidence.json").read_bytes())
        subprocess.run(["python3", "tests/snowbridge/pos-report.py", str(run)], cwd=ROOT, check=True)
    finally:
        with (run / "containers.log").open("w") as output:
            subprocess.run(["docker", "compose", "-f", str(run / "compose.json"), "logs", "--no-color"], stdout=output, stderr=subprocess.STDOUT)
        stack.compose(run, "down")


if __name__ == "__main__":
    main()
