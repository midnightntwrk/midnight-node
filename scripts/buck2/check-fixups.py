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

"""Fail when a fixup value that has to track Cargo.lock has drifted.

A stale value here still builds, so nothing else notices.

Usage: check-fixups.py [Cargo.lock [openssl fixups.toml]]
"""
import sys, tomllib

LOCK = sys.argv[1] if len(sys.argv) > 1 else "Cargo.lock"
FIXUP = sys.argv[2] if len(sys.argv) > 2 else "third-party/fixups/openssl/fixups.toml"

with open(LOCK, "rb") as f:
    versions = [
        p["version"] for p in tomllib.load(f)["package"] if p["name"] == "openssl-src"
    ]
with open(FIXUP, "rb") as f:
    have = tomllib.load(f)["buildscript"]["run"]["env"]["DEP_OPENSSL_VERSION_NUMBER"]

if len(versions) != 1:
    sys.exit(f"error: {LOCK}: expected one openssl-src, found {versions or 'none'}")
# openssl-src is versioned <crate>+<openssl>, and openssl-sys reports
# OPENSSL_VERSION_NUMBER as major << 28 | minor << 20 | patch << 4.
openssl = versions[0].split("+")[1]
major, minor, patch = (int(n) for n in openssl.split("."))
want = f"{major << 28 | minor << 20 | patch << 4:x}"
if have != want:
    sys.exit(
        f"error: {FIXUP}: DEP_OPENSSL_VERSION_NUMBER is {have!r}\n"
        f"  {LOCK} has openssl-src {versions[0]} (OpenSSL {openssl}), which is {want!r}\n"
        f'  fix: set DEP_OPENSSL_VERSION_NUMBER = "{want}", and check DEP_OPENSSL_CONF\n'
        f"       still lists every OPENSSL_NO_* that OpenSSL {openssl} is configured with"
    )
