#!/usr/bin/env python3

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# 	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Fail when a fixup value that has to track Cargo.lock has drifted.

A stale value here still builds, so nothing else notices.

Usage: check-fixups.py [Cargo.lock [openssl fixups.toml]]
"""
import sys, tomllib

LOCK = sys.argv[1] if len(sys.argv) > 1 else "Cargo.lock"
FIXUP = sys.argv[2] if len(sys.argv) > 2 else "third-party/fixups/openssl/fixups.toml"
KEY = "DEP_OPENSSL_VERSION_NUMBER"


def fail(where, what, fix):
    sys.exit(f"error: {where}: {what}\n  fix: {fix}")


with open(LOCK, "rb") as f:
    versions = [
        p["version"] for p in tomllib.load(f)["package"] if p["name"] == "openssl-src"
    ]
if len(versions) != 1:
    fail(
        LOCK,
        f"expected one openssl-src, found {versions or 'none'}",
        "update this check",
    )
# openssl-src is versioned <crate>+<openssl>, and openssl-sys reports
# OPENSSL_VERSION_NUMBER as major << 28 | minor << 20 | patch << 4.
try:
    openssl = versions[0].split("+")[1]
    major, minor, patch = (int(n) for n in openssl.split("."))
except (IndexError, ValueError):
    fail(
        LOCK,
        f"openssl-src {versions[0]} is not <crate>+<major.minor.patch>",
        "update this check",
    )
want = major << 28 | minor << 20 | patch << 4

with open(FIXUP, "rb") as f:
    env = tomllib.load(f).get("buildscript", {}).get("run", {}).get("env", {})
try:
    have = int(env[KEY], 16)
except (KeyError, ValueError):
    fail(
        FIXUP, f"no hexadecimal [buildscript.run.env] {KEY}", f'set {KEY} = "{want:x}"'
    )
if have != want:
    fail(
        FIXUP,
        f"{KEY} is {have:x}, but {LOCK} has openssl-src {versions[0]} "
        f"(OpenSSL {openssl}), which is {want:x}",
        f'set {KEY} = "{want:x}", and check DEP_OPENSSL_CONF still lists every\n'
        f"       OPENSSL_NO_* that OpenSSL {openssl} is configured with",
    )
