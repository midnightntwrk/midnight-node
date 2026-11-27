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

# Group 2, the ledger 8 -> 9 fork's part: the new tooling on the old chain.
# Sourced by groups/g02_old_ledger_features.sh, which has set the scene.

# ECDSA signatures are new in ledger 9. Ledger 8's wire format has none, so the toolkit must
# refuse to build such a transaction while the chain is on ledger 8.
check_new_toolkit_refuses_ecdsa() {
    use_toolkit new
    local log="$EV/ecdsa_on_old_ledger.log"
    if tk_chain generate-txs single-tx --source-seed "ecdsa:$SENDER_SEED" --unshielded-amount 1 \
            --destination-address "$SENDER_ADDRESS" -d "$NODE_WS" > "$log" 2>&1; then
        fail "an ECDSA-signed transfer was accepted on ledger 8"
    elif grep -qiE 'ledger 9|EcdsaNotSupportedForLedger' "$log"; then
        pass "refused by the toolkit: $(grep -m1 -ioE 'EcdsaNotSupportedForLedger\([A-Za-z0-9]*\)|[^"]{0,60}ledger 9[^"]{0,40}' "$log")"
    else
        warn "refused, but not because of the ledger version: $(last_line_of "$log")"
    fi
    use_ledger_side old
}

# Midnight.js 4 refused to deploy with ledger-8 artefacts (the constructor would leave an
# authority nobody can maintain). Midnight.js 5.0.0 accepts it: reported, not judged.
check_midnightjs_old_artefacts_deploy() {
    skip_without_client_stack && return
    local out="$EV/midnightjs_old_artefacts_deploy"
    midnightjs_compile counter out-mjs "$MIDNIGHTJS_OLD_ARTEFACTS_COMPACTC" for-old-ledger > "$EV/midnightjs_compile.log" 2>&1 \
        || { fail "compactc $MIDNIGHTJS_OLD_ARTEFACTS_COMPACTC failed: $(dapp_dir counter)/compile.out-mjs.log"; return; }
    if MN_ZK_VERIFY=warn client_run "$out" mjs_check.mjs retained-deploy --contract-dir "$(dapp_dir counter)/out-mjs" --name counter; then
        warn "Midnight.js deployed a contract from ledger-8 artefacts at $(jq -r .contractAddress "$out.json"); earlier releases refused this"
    elif grep -q Ledger8DeployUnmaintainableError "$out.err"; then
        pass "refused with Ledger8DeployUnmaintainableError"
    else
        fail "$(client_error "$out")"
    fi
}

# A user who switches to the 9.x proof server before the fork: its DUST proofs are refused by
# a ledger-8 node (midnight-ledger#814), so the 8.x proof server must stay until the fork.
check_new_proof_server_on_old_ledger() {
    skip_without_client_stack && return
    local out="$EV/new_proof_server_on_old_ledger" node_error
    if CLIENT_PROOF_SERVER_OLD="$PROOF_SERVER_NEW_URL" client_run "$out" wallet_check.mjs send --kind unshielded --amount 1; then
        pass "accepted: a $PROOF_SERVER_NEW_TAG proof is valid on this ledger-8 node"
        return
    fi
    node_error=$(client_custom_error "$out")
    case "$node_error" in
        170) known "the node refused the proof with Custom error 170, InvalidDustSpendProof ($KNOWN_NEW_PROOF_ON_OLD_LEDGER_ISSUE): keep an 8.x proof server until the fork" ;;
        "")  fail "it failed before the node judged the proof: $(client_error "$out")" ;;
        *)   fail "the node refused it with Custom error $node_error, expected acceptance or 170 (InvalidDustSpendProof)" ;;
    esac
}

check new-toolkit-refuses-ecdsa       "the new toolkit refuses to build an ECDSA-signed transfer on ledger 8"     check_new_toolkit_refuses_ecdsa
check midnightjs-old-artefacts-deploy "a deploy through Midnight.js with ledger-8 artefacts"                      check_midnightjs_old_artefacts_deploy
check new-proof-server-on-old-ledger  "a wallet SDK transfer proved at the 9.x proof server while on ledger 8"    check_new_proof_server_on_old_ledger
