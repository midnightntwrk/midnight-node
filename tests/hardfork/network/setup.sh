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

# Setting up a network, days ahead (table SETUP). On local-env the suite does all of this
# itself.
#
#   setup.sh preflight   are the network, this machine and the wallets ready? Read-only, apart
#                        from filling both toolkit caches: hours the first time on a long
#                        chain, a few blocks after that. Run it until it is all green.
#   setup.sh fund        fresh test wallets get unshielded and shielded NIGHT from FUNDER_SEED
#                        and register for DUST. Run it hours before group 2, so DUST builds
#                        up. Wallets that already have NIGHT, or are registered, are left
#                        alone, so a second run only finishes what is missing.
set -e
export TARGET=network
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

PART="${1:-}"
case "$PART" in
    preflight|fund) ;;
    *) echo "usage: setup.sh preflight|fund" >&2; exit 2 ;;
esac
evidence_dir "setup_$PART"
FUNDED_WALLETS=(SENDER RECEIVER HOLDER SPARE)

# =========================================================================================
if [ "$PART" = preflight ]; then
    group_start SETUP "Preflight on $NETWORK_NAME ($NODE_HTTP)"

    check_node_answers() {
        local spec; spec=$(spec_version)
        if [ -n "$spec" ]; then pass "$(rpc system_chain | jq -r '.result // empty'): node $(node_version), spec $spec, height #$(node_height), finalized #$(finalized_height)"
        else fail "no answer from $NODE_HTTP: check your connection and VPN first"; fi
    }

    check_indexer_answers() {
        local problem
        indexer_resolve || { fail "no GraphQL endpoint answers at $INDEXER_BASE ($INDEXER_GQL_PATHS)"; return; }
        problem=$(indexer_serves_wallet_fields) || { fail "$problem: the wallet SDK cannot sync against $INDEXER_GQL"; return; }
        if problem=$(indexer_catch_up 120); then pass "$INDEXER_GQL is at #$(indexer_height) and serves the fields the wallet SDK needs"
        else warn "$problem"; fi
    }

    # A toolkit cache holds one chain, so a cache filled before a network reset is deleted.
    check_caches_belong_to_this_chain() {
        local genesis side cache cached_genesis
        genesis=$(genesis_hash)
        [ -n "$genesis" ] || { fail "the node reports no genesis hash"; return; }
        [ -z "${EXPECTED_GENESIS_HASH:-}" ] || [ "$genesis" = "$EXPECTED_GENESIS_HASH" ] \
            || { fail "genesis $genesis, the network settings expect $EXPECTED_GENESIS_HASH: was the network reset?"; return; }
        for side in old new; do
            cache=$(toolkit_cache_dir "$side"); mkdir -p "$cache"
            cached_genesis=$(cat "$cache/.genesis" 2> /dev/null || true)
            if [ -n "$cached_genesis" ] && [ "$cached_genesis" != "$genesis" ]; then
                rm -rf "${cache:?}"; mkdir -p "$cache"
                note "the $side toolkit's cache belonged to genesis $cached_genesis: deleted"
            fi
            echo "$genesis" > "$cache/.genesis"
        done
        pass "genesis $genesis"
    }

    check_toolkits_available() {
        local image missing="" reported
        for image in "$OLD_TOOLKIT_IMAGE" "$(new_toolkit_image)"; do
            require_image "$image" 2> /dev/null || missing="$missing $image"
        done
        [ -z "$missing" ] || { fail "missing images:$missing"; return; }
        use_toolkit new
        reported=$(tk_version)
        use_toolkit old
        if [[ "$reported" == *"Node: $NEW_EXPECTED_NODE_VERSION"* && "$reported" == *"Compactc: $NEW_EXPECTED_COMPACTC"* ]]; then pass "$OLD_TOOLKIT_IMAGE; new toolkit: $reported"
        else fail "the new toolkit should be node $NEW_EXPECTED_NODE_VERSION with compactc $NEW_EXPECTED_COMPACTC, it says: $reported"; fi
    }

    # Caches of different toolkit lines are not compatible, so each side has its own.
    check_toolkit_cache_filled() {  # <old|new>
        local role seed seeds=() attempt
        use_toolkit "$1"
        for role in "${FUNDED_WALLETS[@]}"; do seed="${role}_SEED"; seeds+=(--seeds "${!seed}"); done
        for attempt in $(seq 1 12); do
            if tk_chain fetch "${seeds[@]}" > "$EV/fill_cache_$1.log" 2>&1; then
                pass "the $1 toolkit's cache at $TK_CACHE_DIR is filled to #$(node_height) (attempt $attempt)"
                use_toolkit old
                return
            fi
            sleep 10
        done
        use_toolkit old
        fail "$(last_line_of "$EV/fill_cache_$1.log")"
    }

    check_clients_ready() {
        proof_server_up old > "$EV/proof_server_old.log" 2>&1 && proof_server_up new > "$EV/proof_server_new.log" 2>&1 \
            || { fail "a proof server could not be started or reached ($EV/proof_server_*.log)"; return; }
        clients_ready > "$EV/clients_ready.log" 2>&1 || { fail "npm ci in $CLIENT_PACKAGES_DIR failed ($EV/clients_ready.log)"; return; }
        pass "proof servers $(proof_server_version "$PROOF_SERVER_OLD_URL") and $(proof_server_version "$PROOF_SERVER_NEW_URL"); installed: $(clients_versions)"
    }

    bullet "Run preflight until it is all green: endpoints, images, caches, proof servers, clients"
    check preflight-node-answers                 "the node's RPC answers"                                          check_node_answers
    check preflight-indexer-answers              "the indexer answers, follows the tip and serves the wallet SDK"  check_indexer_answers
    check preflight-caches-belong-to-this-chain  "the toolkit caches belong to this chain's genesis"               check_caches_belong_to_this_chain
    check preflight-toolkits-available           "both toolkit images are available and the new one is the release" check_toolkits_available
    check preflight-old-toolkit-cache-filled     "the old toolkit's cache is filled for the test wallets"          check_toolkit_cache_filled old
    check preflight-new-toolkit-cache-filled     "the new toolkit's cache is filled for the test wallets"          check_toolkit_cache_filled new
    check preflight-clients-ready                "the proof servers and the client packages are ready"             check_clients_ready
    group_finish
    exit $?
fi

# =========================================================================================
# fund
use_ledger_side old
use_wallets genesis
require_chain_on old
is_hex64 "${FUNDER_SEED:-}" || { echo "FUNDER_SEED (64 hex characters) is missing in $SEEDS_FILE" >&2; exit 2; }
NIGHT_PER_WALLET="${NIGHT_PER_WALLET:?NIGHT_PER_WALLET, in atomic units (1 NIGHT = 10^6), in the network settings}"

group_start SETUP "Test wallets funded from FUNDER_SEED on $NETWORK_NAME ($NIGHT_PER_WALLET atomic units each)"

check_funder_can_pay() {
    local needed=$(( NIGHT_PER_WALLET * ${#FUNDED_WALLETS[@]} )) unshielded shielded dust
    wallet_snapshot "$FUNDER_SEED" "$EV/funder_wallet.json" || { fail "the funder's wallet is unreadable"; return; }
    dust_snapshot "$FUNDER_SEED" "$EV/funder_dust.json" || { fail "the funder's DUST balance is unreadable"; return; }
    unshielded=$(wallet_night_unshielded "$EV/funder_wallet.json"); shielded=$(wallet_night_shielded "$EV/funder_wallet.json")
    dust=$(jq -r .total "$EV/funder_dust.json")
    big_gt "$unshielded" "$needed" || { fail "the funder has $unshielded unshielded NIGHT, it needs more than $needed"; return; }
    big_gt "$shielded" "$needed" || { fail "the funder has $shielded shielded NIGHT, it needs more than $needed"; return; }
    big_gt "$dust" 0 || { fail "the funder has no DUST to pay fees with"; return; }
    pass "the funder has $unshielded unshielded and $shielded shielded NIGHT, and DUST"
}

check_wallet_funded() {  # <role>
    local role=$1 seed="${1}_SEED" address="${1}_ADDRESS" shielded_address="${1}_SHIELDED_ADDRESS" did=""
    wallet_snapshot "${!seed}" "$EV/${role}_before.json" || { fail "the $role wallet is unreadable"; return; }
    if ! big_gt "$(wallet_night_unshielded "$EV/${role}_before.json")" 0; then
        send_night "fund_${role}_unshielded" --source-seed "$FUNDER_SEED" --unshielded-amount "$NIGHT_PER_WALLET" --destination-address "${!address}" > /dev/null \
            || { fail "unshielded NIGHT for the $role: $(last_line_of "$EV/fund_${role}_unshielded.log")"; return; }
        did="$did unshielded NIGHT sent;"
    fi
    if ! big_gt "$(wallet_night_shielded "$EV/${role}_before.json")" 0; then
        send_night "fund_${role}_shielded" --source-seed "$FUNDER_SEED" --shielded-amount "$NIGHT_PER_WALLET" --destination-address "${!shielded_address}" > /dev/null \
            || { fail "shielded NIGHT for the $role: $(last_line_of "$EV/fund_${role}_shielded.log")"; return; }
        did="$did shielded NIGHT sent;"
    fi
    dust_snapshot "${!seed}" "$EV/${role}_dust.json" || { fail "the $role wallet's DUST balance is unreadable"; return; }
    if [ "$(jq '.source | length' "$EV/${role}_dust.json")" = 0 ]; then
        register_for_dust "${!seed}" "$FUNDER_SEED" "$EV/register_$role.log" || { fail "registering the $role for DUST: $(last_line_of "$EV/register_$role.log")"; return; }
        did="$did registered for DUST, the funder paid;"
    fi
    pass "${did:- already funded and registered}"
}

bullet "Create fresh test wallets and fund them early"
check fund-funder-can-pay "the funder has enough NIGHT, and DUST" check_funder_can_pay
for role in "${FUNDED_WALLETS[@]}"; do
    role_name=$(echo "$role" | tr 'A-Z' 'a-z')
    check "fund-$role_name-wallet" "the $role_name has unshielded and shielded NIGHT and is registered for DUST" check_wallet_funded "$role"
done
group_finish
