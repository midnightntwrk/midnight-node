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

# Known issues of the ledger 8 -> 9 fork. A check reports KNOWN only when its evidence
# matches one of these; anything else that goes wrong is a FAIL.

# node#1969 (closed as intended): a contract's ledger-8 artefacts cannot build a ledger-9
# transaction through toolkit-js; recompiled artefacts can.
KNOWN_OLD_ARTEFACTS_ERROR='Version mismatch: compiled code expects 0\.15\.[0-9]+, runtime is 0\.(1[89]|[2-9][0-9])\.|RuntimeApiError\(IncompatibleCodegen\)'
KNOWN_OLD_ARTEFACTS_ISSUE="node#1969, closed as intended"

# midnight-ledger#811: a ledger-8 binary can panic in its storage core. Fixed in ledger 9.
KNOWN_OLD_BINARY_PANIC='panicked at .attempted to increment non-existant ref'
KNOWN_OLD_BINARY_PANIC_ISSUE="midnight-ledger#811"

# midnight-ledger#814: a ledger-8 node refuses DUST proofs made by a 9.x proof server.
KNOWN_NEW_PROOF_ON_OLD_LEDGER_ISSUE="midnight-ledger#814"

# midnight-indexer#1604: the indexer records a transaction that safe mode refused as applied.
# A wallet SDK that reads its DUST from that indexer then spends DUST it does not have, and
# the node refuses the proof with this error.
KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ISSUE="midnight-indexer#1604"
KNOWN_INDEXER_SHOWS_REFUSED_AS_APPLIED_ERROR=170

# midnight-indexer#1605: for a contract untouched since the fork the indexer served its last
# pre-fork state. Fixed in #1654, which is a required fix (required-fixes.tsv), so by default
# this is a failure. INDEXER_HAS_STALE_STATE_FIX=0 is for an indexer from before the fix.
KNOWN_INDEXER_STALE_STATE_ISSUE="midnight-indexer#1605"
known_indexer_stale_state() {  # <address> <evidence prefix of the node-vs-indexer comparison> <fork height>
    local node_data indexer_data
    [ "${INDEXER_HAS_STALE_STATE_FIX:-1}" = 0 ] || return 1
    # Known only when the indexer serves exactly the bytes of the block before the fork,
    # and they hold the same data as the node's current state.
    [ "$(state_tag "$(cat "$2.node.hex")")" = "contract-state[v8]" ] || return 1
    [ "$(state_tag "$(cat "$2.indexer.hex")")" = "contract-state[v6]" ] || return 1
    contract_state_at "$1" "$(block_hash_at "$NODE_HTTP" $(( $3 - 1 )))" > "$2.before_fork.hex"
    cmp -s "$2.before_fork.hex" "$2.indexer.hex" || return 1
    node_data=$(contract_data "$2.node.hex") && indexer_data=$(contract_data "$2.indexer.hex") || return 1
    [ "$(cut -d' ' -f2 <<< "$node_data")" = "$(cut -d' ' -f2 <<< "$indexer_data")" ]
}
