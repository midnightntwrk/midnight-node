#!/usr/bin/env bash
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

# The ledger 8 -> 9 hard fork: the release matrix. One setting per version; every value can
# be overridden from the environment. "old" is the ledger the chain forks from, "new" the
# one it forks to.

FORK_NAME="8-to-9"          # names the work directory, target/hardfork-<name>
OLD_LEDGER=8
NEW_LEDGER=9

# --- The node on each side ------------------------------------------------------------
# A node-<version> ref selects the release images; any other ref (a branch, a commit) the
# CI images of its checkout.
OLD_REF="${OLD_REF:-node-1.0.400}"
OLD_EXPECTED_NODE_VERSION="${OLD_EXPECTED_NODE_VERSION:-1.0.400}"
OLD_EXPECTED_SPEC="${OLD_EXPECTED_SPEC:-1000300}"
OLD_EXPECTED_LEDGER="${OLD_EXPECTED_LEDGER:-=8.1.3}"       # as midnight_ledgerVersion reports it
OLD_EXPECTED_COMPACTC="${OLD_EXPECTED_COMPACTC:-0.30.0}"   # the compiler the old toolkit bundles

NEW_REF="${NEW_REF-node-2.1.101}"
NEW_EXPECTED_NODE_VERSION="${NEW_EXPECTED_NODE_VERSION:-2.1.101}"
NEW_EXPECTED_SPEC="${NEW_EXPECTED_SPEC:-2001100}"
NEW_EXPECTED_LEDGER="${NEW_EXPECTED_LEDGER:-1.0.1}"        # also the npm version of that ledger build
NEW_EXPECTED_LEDGER_TAG="${NEW_EXPECTED_LEDGER_TAG:-crate-ledger-9.1.0.1}"
NEW_EXPECTED_COMPACTC="${NEW_EXPECTED_COMPACTC:-0.35.0}"

# Old-ledger runtime specs are below this, new-ledger specs at or above it.
NEW_SPEC_FLOOR=2000000

# --- Services -------------------------------------------------------------------------
# The indexer must be the build made for the new node: it stops at the fork block otherwise.
INDEXER_TAG="${INDEXER_TAG:-4.4.101}"
PROOF_SERVER_OLD_TAG="${PROOF_SERVER_OLD_TAG:-8.1.3}"
PROOF_SERVER_NEW_TAG="${PROOF_SERVER_NEW_TAG:-9.0.1}"

# --- Client libraries -----------------------------------------------------------------
# Their versions are pinned in clients/package.json (the wallet SDK and Midnight.js) and in
# clients-legacy/package.json (the wallet SDK from before the fork). These are the installed
# packages the checks report, as <name>=<npm package>.
CLIENT_PACKAGES=(
    "wallet-sdk=@midnightntwrk/wallet-sdk"
    "midnight-js=@midnight-ntwrk/midnight-js-contracts"
    "ledger-v8=@midnightntwrk/ledger-v8"
    "ledger-v9=@midnightntwrk/ledger-v9"
    "compact-js=@midnight-ntwrk/compact-js"
    "compact-runtime=@midnight-ntwrk/compact-runtime"
)
# Midnight.js bundles its own compact-runtime, so it needs artefacts from the compiler that
# targets that runtime, which is not the one the toolkit bundles.
MIDNIGHTJS_OLD_ARTEFACTS_COMPACTC="${MIDNIGHTJS_OLD_ARTEFACTS_COMPACTC:-0.31.1}"   # for contracts deployed before the fork
MIDNIGHTJS_NEW_ARTEFACTS_COMPACTC="${MIDNIGHTJS_NEW_ARTEFACTS_COMPACTC:-0.36.0}"   # for contracts deployed after it
MIDNIGHTJS_OLD_RUNTIME_ALIAS="compact-runtime-ledger8"   # the name old artefacts import compact-runtime by

# --- How the new node refuses what it must refuse (group 8) ----------------------------
# The numbers of the node's custom transaction errors.
NODE_ERROR_CANNOT_DECODE=1
declare -gA NODE_ERRORS_FOR_OLD_PROOF=([170]=InvalidDustSpendProof [179]=UnsupportedProofVersion)

# --- dApps only this fork has (dapps/) --------------------------------------------------
declare -gA DAPP_COMPACTC_FLAGS=([keccak]="--feature-zkir-v3")
declare -gA DAPP_WITHOUT_WITNESSES=([events]=EventsContract [keccak]=KeccakContract [ccc-outer]=CccOuterContract)
