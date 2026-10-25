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

"""Native-only compatibility experiment against saved live BEEFY captures.

Reads ../bridge-test without modifying it. This does not generate an SP1 proof.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
BRIDGE = ROOT.parent / "bridge-test"
ART = ROOT / "artifacts/snowbridge/sp1-feasibility"
run = Path(sys.argv[1]).resolve()
assert run.is_relative_to(ROOT / "artifacts/snowbridge/ethereum-pos")
manifest = json.loads((run / "validation.json").read_text())
assert manifest["completeBridgeE2E"]
for name, expected in manifest["sha256"].items():
    assert hashlib.sha256((run / name).read_bytes()).hexdigest() == expected, name
revision = subprocess.check_output(["git", "-C", str(BRIDGE), "rev-parse", "HEAD"], text=True).strip()
assert revision == "e64f7f285545fe15fa94b80853ed374d9595d878", "Re-review a different example revision explicitly"
source_path = BRIDGE / "crates/beefy/src/lib.rs"
source = source_path.read_text()
committed = subprocess.check_output(["git", "-C", str(BRIDGE), "show", "HEAD:crates/beefy/src/lib.rs"])
assert source.encode() == committed, "Verifier source changed; re-review before adapting"
old = """        let mut leaf = [0u8; 41];
        leaf[..33].copy_from_slice(key);
        leaf[33..].copy_from_slice(&stake.to_le_bytes());"""
new = """        // Explicit profile for ethereum-bridge-poc; never auto-detect from the witness.
        let public = VerifyingKey::from_sec1_bytes(key).unwrap().to_encoded_point(false);
        let digest = keccak(&public.as_bytes()[1..]);
        let mut leaf = [0u8; 28];
        leaf[..20].copy_from_slice(&digest[12..]);
        leaf[20..].copy_from_slice(&stake.to_le_bytes());"""
assert source.count(old) == 1
adapted = source.replace(old, new)
for folder in [ART / "adapted-beefy/src", ART / "check/src"]:
    folder.mkdir(parents=True, exist_ok=True)
(ART / "adapted-beefy/src/lib.rs").write_text(adapted)
cargo = (BRIDGE / "crates/beefy/Cargo.toml").read_text()
(ART / "adapted-beefy/Cargo.toml").write_text(cargo.replace('name = "midnight-beefy"', 'name = "midnight-beefy-poc-compat"'))
(ART / "check/src/main.rs").write_bytes((ROOT / "tests/snowbridge/sp1-compat/main.rs").read_bytes())
(ART / "check/Cargo.toml").write_text(f"""[package]
name = "midnight-sp1-feasibility-check"
version = "0.1.0"
edition = "2021"
[workspace]
[dependencies]
midnight-beefy = {{ path = "{BRIDGE}/crates/beefy" }}
midnight-beefy-poc-compat = {{ path = "../adapted-beefy" }}
parity-scale-codec = {{ version = "=3.7.5", features = ["derive"] }}
k256 = {{ version = "=0.13.4", features = ["ecdsa"] }}
serde_json = "1"
hex = "0.4"
""")
command = ["cargo", "run", "--manifest-path", str(ART / "check/Cargo.toml")]
if (ART / "check/Cargo.lock").exists():
    command.append("--locked")
result = subprocess.run([*command, "--", str(run)], text=True, capture_output=True)
if result.returncode:
    print(result.stderr, file=sys.stderr)
    result.check_returncode()
evidence = json.loads(result.stdout)
evidence["exampleRevision"] = revision
evidence["sourceSha256"] = hashlib.sha256(source.encode()).hexdigest()
evidence["adaptedSourceSha256"] = hashlib.sha256(adapted.encode()).hexdigest()
evidence["lockSha256"] = hashlib.sha256((ART / "check/Cargo.lock").read_bytes()).hexdigest()
(ART / "native-compatibility.json").write_text(json.dumps(evidence, indent=2) + "\n")
print(f"PASS: {len(evidence['adaptedNativeAccepted'])} live captures, {evidence['negativeChecks']} negative checks; native only")
print(ART / "native-compatibility.json")
