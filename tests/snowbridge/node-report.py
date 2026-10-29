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

"""Record completed node integration stages; never label the smoke test as bridge E2E."""
import hashlib
import json
from pathlib import Path
from datetime import datetime, timezone
ROOT = Path(__file__).resolve().parents[2]
ART = ROOT / "artifacts/snowbridge"

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

sources = json.loads((ART / "node-build-sources.json").read_text())
for name, expected in sources["files"].items():
    assert sha(ROOT / name) == expected, f"Source changed since staging: {name}"
for filename in ["pallet-tests.log", "host-tests.log", "genesis-test.log", "system-regression.log", "runtime-filter-tests.log"]:
    assert "test result: ok." in (ART / filename).read_text(), filename
assert "Finished `release`" in (ART / "node-build.log").read_text(), "Release/Wasm build incomplete"
smoke = json.loads((ART / "node-smoke-evidence.json").read_text())
assert smoke["finalizedNumber"] >= 3
assert smoke["binarySha256"] == sha(ROOT / "target/release/midnight-node")
manifest = {"createdAt": datetime.now(timezone.utc).isoformat(), "completeBridgeE2E": False,
    "liveMidnightConsensus": True, "liveEthereumBeaconFinality": False,
    "sources": sources, "smoke": smoke,
    "harness": {name: sha(ROOT / "tests/snowbridge" / name) for name in ["node-test.sh", "node-smoke.py", "node-report.py", "prepare-node.py"]},
    "logs": {name: sha(ART / name) for name in ["pallet-tests.log", "host-tests.log", "genesis-test.log", "node-build.log", "system-regression.log", "runtime-filter-tests.log"]},
    "patches": {name: sha(ROOT / "tests/snowbridge/patches" / name) for name in ["ledger.patch", "migration.patch"]}}
(ART / "node-validation.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Node integration and live block production passed. Cross-chain E2E remains unvalidated.")
