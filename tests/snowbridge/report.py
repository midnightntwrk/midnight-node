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

"""Collect reproducibility evidence after both validation stages have passed."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / "artifacts/snowbridge"

def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def files_under(directory):
    for current, directories, files in os.walk(directory):
        directories[:] = [d for d in directories if d not in {"node_modules", "broadcast", "__pycache__", "target", ".git"}]
        for name in sorted(files):
            path = Path(current) / name
            yield str(path.relative_to(ROOT)), {"sha256": digest(path), "bytes": path.stat().st_size}

ledger = json.loads((ART / "compact-ledger/evidence.json").read_text())
live = json.loads((ART / "live-evidence.json").read_text())
assert ledger["committedEscrowDecoded"]
assert ledger["stateRoot"] == ledger["replayedStateRoot"]
assert live["custodyBalance"] == 60 and live["recipientBalance"] == 40
assert live["dispatchSuccess"] == [True, False]
for checkout, patch in [("ledger", "ledger.patch"), ("upstream", "upstream.patch"), ("migration", "migration.patch")]:
    actual = subprocess.check_output(["git", "-C", str(ART / checkout), "diff"], cwd=ROOT)
    assert actual == (ROOT / "tests/snowbridge/patches" / patch).read_bytes(), f"Unrecorded {checkout} source changes"

manifest = {
    "createdAt": datetime.now(timezone.utc).isoformat(),
    "profile": "independent-ethereum-execution-and-funded-ledger-fixtures",
    "completeBridgeE2E": False,
    "liveMidnightConsensus": False,
    "ethereumBeaconFinalityVerified": False,
    "sources": {
        "node": command("git", "rev-parse", "HEAD"),
        "branch": command("git", "branch", "--show-current"),
        "snowbridge": command("git", "-C", str(ART / "upstream"), "rev-parse", "HEAD"),
        "ledger": command("git", "-C", str(ART / "ledger"), "rev-parse", "HEAD"),
        "sdk": "9c7455eabd0cc2e77da867e837dfb466b11fbcf6",
        "migration": command("git", "-C", str(ART / "migration"), "rev-parse", "HEAD"),
        "soliditySubmodules": command("git", "-C", str(ART / "upstream"), "submodule", "status"),
    },
    "versions": {
        "rustc": command("rustc", "--version"),
        "node": command("node", "--version"),
        "compact": command(os.environ.get("COMPACTC", str(ART / "tools/compact/compactc")), "--version"),
        "foundry": command(os.environ.get("FORGE", str(ART / "tools/foundry/forge")), "--version"),
        "solidity": "0.8.34",
    },
    "fixtureSources": dict(files_under(ROOT / "tests/snowbridge")),
    "compiledCompact": dict(files_under(ART / "compact")),
    "evidence": {str(path.relative_to(ROOT)): digest(path) for path in [
        ART / "live-evidence.json", ART / "deposit-logs.json",
        ART / "compact-ledger/evidence.json", ART / "compact-ledger/escrow-state.bin",
        ART / "compact-ledger/issuance.bin", *sorted((ART / "compact-ledger").glob("proven-*.bin")),
    ]},
}
(ART / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(f"Evidence manifest: {ART / 'manifest.json'}")
print("Both component fixtures passed. Complete bridge E2E: NOT VALIDATED.")
