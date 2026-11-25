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

# The feature checks that run on both sides of the fork: in group 2 on the old ledger with
# the wallets of the seeds file, and again in group 7 on the new ledger with fresh wallets,
# contracts and dApps. Sourced by both; use_ledger_side and use_wallets set the scene first.
#
# Contracts are recorded by name. Group 7 sets CONTRACT_PREFIX=fresh-, so its counter is
# fresh-counter and group 2's stays counter.

# --- Transfers ------------------------------------------------------------------------
check_transfer_unshielded() {
    local problem
    problem=$(send_night transfer_unshielded --source-seed "$SENDER_SEED" --unshielded-amount 10 \
        --destination-address "$RECEIVER_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_shielded() {
    local problem
    problem=$(send_night transfer_shielded --source-seed "$SENDER_SEED" --shielded-amount 10 \
        --destination-address "$RECEIVER_SHIELDED_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_mixed() {
    local problem
    problem=$(send_night transfer_mixed --source-seed "$SENDER_SEED" --shielded-amount 5 --unshielded-amount 5 \
        --destination-address "$SPARE_SHIELDED_ADDRESS" --destination-address "$SPARE_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_two_destinations() {
    local problem
    problem=$(send_night transfer_two_destinations --source-seed "$SENDER_SEED" --unshielded-amount 5 \
        --destination-address "$RECEIVER_ADDRESS" --destination-address "$SPARE_ADDRESS") && pass "landed" || fail "$problem"
}

check_transfer_from_second_wallet() {
    local problem
    problem=$(send_night transfer_from_second_wallet --source-seed "$RECEIVER_SEED" --unshielded-amount 10 \
        --destination-address "$SENDER_ADDRESS") && pass "landed" || fail "$problem"
}

# --- DUST -----------------------------------------------------------------------------
# A wallet that just received NIGHT has no DUST yet, so another wallet pays for its
# registration: REGISTRATION_PAYER[<role>] names its seed. Each wallet needs a payer of its
# own, and not the funder: a wallet that has just paid a fee cannot pay for a registration
# right away (the toolkit refuses to build it, OutsideTimeToDismiss).
declare -A REGISTRATION_PAYER
check_dust_registration() {  # <funder seed> <atomic units per UTXO> <role>...
    local funder=$1 amount=$2 role seed address
    shift 2
    for role in "$@"; do
        seed="${role}_SEED"; address="${role}_ADDRESS"
        wallet_snapshot "${!seed}" "$EV/${role}_before_funding.json" || { fail "$role: wallet unreadable"; return; }
        if [ "$(night_utxo_count "$EV/${role}_before_funding.json")" -lt 3 ]; then
            fund_with_night "$funder" "${!address}" "$amount" "$EV/${role}_funding.log" \
                || { fail "funding the $role: $(last_line_of "$EV/${role}_funding.log")"; return; }
        fi
        dust_snapshot "${!seed}" "$EV/${role}_dust_before.json" || { fail "$role: DUST balance unreadable"; return; }
        if [ "$(jq '.source | length' "$EV/${role}_dust_before.json")" = 0 ]; then
            register_for_dust "${!seed}" "${REGISTRATION_PAYER[$role]}" "$EV/${role}_registration.log" \
                || { fail "registering the $role: $(last_line_of "$EV/${role}_registration.log")"; return; }
        fi
    done
    pass "funded with three NIGHT UTXOs of $amount atomic units and registered for DUST: $*"
}

check_dust_generating() {  # <role>...
    local role seed growth report=""
    for role in "$@"; do
        seed="${role}_SEED"
        growth=$(dust_growth_check "${!seed}" "$EV/dust_growth_$role") || { fail "$role: no DUST source grew: $growth"; return; }
        report="$report $role: $growth;"
    done
    pass "$report"
}

# --- Contracts ------------------------------------------------------------------------
check_contract_store_a() {
    local label address problem
    label=$(contract_label store-a)
    address=$(store_contract_deploy "$label" "$SENDER_SEED") || { fail "deploy: $address"; return; }
    problem=$(store_contract_call "$label" "$SENDER_SEED" store check) || { fail "$label at $address: $problem"; return; }
    pass "$label at $address: deployed, stored a value, checked it"
}

check_contract_store_b_committee() {
    local label address problem
    label=$(contract_label store-b)
    address=$(store_contract_deploy "$label" "$RECEIVER_SEED" \
        --authority-seed "$SENDER_SEED" --authority-seed "$RECEIVER_SEED" --authority-threshold 1) || { fail "deploy: $address"; return; }
    problem=$(store_contract_call "$label" "$RECEIVER_SEED" store check) || { fail "$label at $address: $problem"; return; }
    pass "$label at $address: committee of sender and receiver, threshold 1; stored a value, checked it"
}

check_contract_authority_rotated() {
    local label problem
    label=$(contract_label store-b)
    [ -n "$(registry_address "$label")" ] || { skip "no contract $label"; return; }
    problem=$(store_contract_rotate_authority "$label" "$RECEIVER_SEED" "$SENDER_SEED" "$RECEIVER_SEED" "$HOLDER_SEED") \
        || { fail "$problem"; return; }
    pass "the sender signed; the committee is now receiver and holder; the contract's counter went up by one"
}

# --- dApps ----------------------------------------------------------------------------
deploy_dapp() {  # <name> -> address
    local address
    address=$(dapp_deploy "$1" "$SENDER_SEED") || { echo "deploy: $address; $(dapp_err "$1" deploy)"; return 1; }
    registry_add "$(contract_label "$1")" dapp "$address" "$SIDE"
    echo "$address"
}

check_dapp_counter() {
    local address
    address=$(deploy_dapp counter) || { fail "$address"; return; }
    dapp_call counter "$SENDER_SEED" "$address" increment || { fail "first increment: $(dapp_err counter increment)"; return; }
    dapp_call counter "$SENDER_SEED" "$address" increment || { fail "second increment: $(dapp_err counter increment)"; return; }
    pass "counter at $address: deployed, incremented twice; private count $(jq -r .count "$(dapp_ps_file counter)")"
}

# bboard's poster is derived from private state, which take_down needs later.
check_dapp_bboard() {
    local address
    address=$(deploy_dapp bboard) || { fail "$address"; return; }
    dapp_call bboard "$SENDER_SEED" "$address" post "posted-on-ledger-$LEDGER" || { fail "post: $(dapp_err bboard post)"; return; }
    pass "bboard at $address: deployed, a message posted"
}

# micro-dao has the richest ledger state of the dApps here.
check_dapp_micro_dao() {
    local address entry_points
    address=$(deploy_dapp micro-dao) || { fail "$address"; return; }
    entry_points=$(contract_semantic_state "$address" | grep -c '^op=' || true)
    if [ "$entry_points" -ge 5 ]; then pass "micro-dao at $address: deployed, $entry_points entry points"
    else fail "micro-dao at $address has $entry_points entry points, expected at least 5"; fi
}

check_dapp_vault_mints_and_pays() {
    local address token problem
    address=$(deploy_dapp vault) || { fail "$address"; return; }
    problem=$(vault_call mint 1000000) || { fail "$problem"; return; }
    problem=$(vault_call pay_out "$(wallet_address "$SENDER_SEED" user-address)" 1000) || { fail "$problem"; return; }
    token=$(contract_balances "$address" | jq -r 'keys[0] // empty')
    [ -n "$token" ] || { fail "the vault holds no token after minting"; return; }
    remember "$(state_key "$(contract_label vault)" TOKEN)" "$token"
    problem=$(vault_must_hold 999000) || { fail "$problem"; return; }
    pass "vault at $address: minted 1000000 of its token into itself, paid 1000 to the sender, holds 999000"
}

# A whole game: both stake, both reveal rock, the draw refunds both.
check_game_draw_played() {
    local address host_before host_after problem
    address=$(game_deploy draw) || { fail "$address"; return; }
    host_before=$(unshielded_night_of "$SENDER_SEED" draw_host_before) || { fail "$host_before"; return; }
    problem=$(game_start draw "$ROCK" "$ROCK") || { fail "$problem"; return; }
    problem=$(game_call draw host reveal) || { fail "$problem"; return; }
    problem=$(game_call draw guest reveal) || { fail "$problem"; return; }
    problem=$(game_call draw guest settle) || { fail "$problem"; return; }
    host_after=$(unshielded_night_of "$SENDER_SEED" draw_host_after) || { fail "$host_after"; return; }
    gained_exactly "$host_before" "$host_after" 0 || { fail "a draw should refund the host: its NIGHT went $host_before -> $host_after"; return; }
    problem=$(contract_must_hold_night "$address" 0) || { fail "$problem"; return; }
    pass "game at $address: both staked $GAME_STAKE, rock against rock, settled; both refunded, the contract holds nothing"
}

# --- Versions and the chain -------------------------------------------------------------
check_ledger_version() {
    local reported; reported=$(ledger_version)
    if [ "$reported" = "$EXPECTED_LEDGER_VERSION" ]; then pass "$reported"
    else fail "the node reports '$reported', expected $EXPECTED_LEDGER_VERSION"; fi
}

check_runtime_spec() {
    local spec; spec=$(spec_version)
    if [ "$spec" = "$EXPECTED_SPEC" ]; then pass "spec $spec"
    else fail "spec $spec, expected $EXPECTED_SPEC"; fi
}

check_toolkit_version() {
    local reported; reported=$(tk_version)
    echo "$reported" > "$EV/toolkit_version.txt"
    if [[ "$reported" == *"Node: $EXPECTED_NODE_VERSION"* && "$reported" == *"Compactc: $EXPECTED_COMPACTC"* ]]; then pass "$reported"
    else fail "expected node $EXPECTED_NODE_VERSION with compactc $EXPECTED_COMPACTC, the toolkit says: $reported"; fi
}

# --- The indexer ------------------------------------------------------------------------
check_indexer_has_contracts() {
    local address total=0 missing=""
    for address in $(registry_addresses "$SIDE"); do
        total=$(( total + 1 ))
        [ -n "$(indexer_contract_type "$address")" ] || missing="$missing $(registry_label "$address")"
    done
    if [ "$total" = 0 ]; then fail "no contract was deployed on this side"
    elif [ -n "$missing" ]; then fail "not in the indexer:$missing"
    else pass "$total contracts deployed on ledger $LEDGER are in the indexer"; fi
}

# Every transaction this group sent, looked up by hash.
check_indexer_has_transactions() {
    local report
    if report=$(tx_inclusion_check "$EV/tx_inclusion.tsv" "$EV"/*.log "$DAPPS_DIR"/*/"${DAPP_FILE_PREFIX}"_*send.log); then pass "$report"
    else fail "$report ($EV/tx_inclusion.tsv)"; fi
}

# --- The client stack: proof server, wallet SDK, Midnight.js ------------------------------
CLIENT_STACK_READY=0
# First line of a check that needs the stack:  skip_without_client_stack && return
skip_without_client_stack() {
    [ "$CLIENT_STACK_READY" = 1 ] && return 1
    skip "the client stack is not ready (see ${GROUP}-client-stack-ready)"
}

check_client_stack_ready() {
    local old_version new_version problem installed
    proof_server_up old > "$EV/proof_server_old.log" 2>&1 && proof_server_up new > "$EV/proof_server_new.log" 2>&1 \
        || { fail "a proof server could not be started or reached ($EV/proof_server_*.log)"; return; }
    old_version=$(proof_server_version "$PROOF_SERVER_OLD_URL"); new_version=$(proof_server_version "$PROOF_SERVER_NEW_URL")
    [[ "$old_version" == "$OLD_LEDGER".* && "$new_version" == "$NEW_LEDGER".* ]] \
        || { fail "proof servers report '$old_version' and '$new_version', expected a $OLD_LEDGER.x and a $NEW_LEDGER.x"; return; }
    problem=$(indexer_serves_wallet_fields) || { fail "$problem: the wallet SDK cannot sync against this indexer"; return; }
    clients_ready > "$EV/clients_ready.log" 2>&1 || { fail "npm ci in $CLIENT_PACKAGES_DIR failed ($EV/clients_ready.log)"; return; }
    installed=$(clients_versions)
    [ "$(client_version wallet-sdk "$installed")" = "$(clients_pinned @midnightntwrk/wallet-sdk)" ] \
        && [ "$(client_version midnight-js "$installed")" = "$(clients_pinned @midnight-ntwrk/midnight-js-contracts)" ] \
        || { fail "installed $installed differs from $CLIENT_PACKAGES_DIR/package.json"; return; }
    CLIENT_STACK_READY=1
    pass "proof servers $old_version and $new_version; the indexer serves the wallet SDK's fields; installed: $installed"
}

check_clients_read_chain_era() {
    skip_without_client_stack && return
    local sdk_era midnightjs_era
    client_run "$EV/sdk_probe" wallet_check.mjs probe || { fail "wallet SDK: $(client_error "$EV/sdk_probe")"; return; }
    client_run "$EV/midnightjs_head" mjs_check.mjs head || { fail "Midnight.js: $(client_error "$EV/midnightjs_head")"; return; }
    sdk_era=$(jq -r .eraForHead "$EV/sdk_probe.json"); midnightjs_era=$(jq -r .era "$EV/midnightjs_head.json")
    if [ "$sdk_era" = "$SDK_ERA" ] && [ "$midnightjs_era" = "$SDK_ERA" ]; then
        pass "both read protocol version $(jq -r .chainHead.protocolVersion "$EV/sdk_probe.json") as $SDK_ERA"
    else fail "the wallet SDK reads the head as $sdk_era, Midnight.js as $midnightjs_era, expected $SDK_ERA"; fi
}

sdk_unshielded_balance() {  # <sync output.json> [token type]
    jq -r --arg token "${2:-$NIGHT_TOKEN_TYPE}" '.wallet.unshielded.balances[$token] // "0"' "$1"
}

check_sdk_wallet_syncs() {
    skip_without_client_stack && return
    local night dust_coins
    client_run "$EV/sdk_sync" wallet_check.mjs sync || { fail "$(client_error "$EV/sdk_sync")"; return; }
    night=$(sdk_unshielded_balance "$EV/sdk_sync.json"); dust_coins=$(jq -r '.wallet.dust.availableCoins // 0' "$EV/sdk_sync.json")
    big_gt "$night" 0 || { fail "synced, but the wallet shows no NIGHT"; return; }
    [ "$dust_coins" -gt 0 ] || { fail "synced with NIGHT $night, but no DUST coin to pay fees with"; return; }
    pass "the sender's wallet synced through the indexer: NIGHT $night, $dust_coins DUST coins"
}

sdk_transfer() {  # <shielded|unshielded>: the check body of both SDK transfers
    skip_without_client_stack && return
    local out="$EV/sdk_transfer_$1"
    if client_run "$out" wallet_check.mjs send --kind "$1" --amount 1; then
        pass "transaction $(jq -r .txId "$out.json" | cut -c1-20)... proved at protocol version $(jq -r .provedAtProtocolVersion "$out.json") and accepted"
    else fail "$(client_error "$out")"; fi
}
check_sdk_transfer_unshielded() { sdk_transfer unshielded; }
check_sdk_transfer_shielded()   { sdk_transfer shielded; }

check_sdk_history() {
    skip_without_client_stack && return
    local history entries
    client_run "$EV/sdk_history" wallet_check.mjs sync || { fail "$(client_error "$EV/sdk_history")"; return; }
    history=$(jq -c '.history // empty' "$EV/sdk_history.json")
    [ -n "$history" ] || { fail "the wallet's sync output has no history"; return; }
    entries=$(jq -r ".byEra.$SDK_ERA // 0" <<< "$history")
    if [ "$entries" -gt 0 ]; then pass "$entries ledger-$LEDGER entries: $history"
    else fail "no ledger-$LEDGER entries in the history: $history"; fi
}

# The addresses a user would paste: printed by the toolkit, parsed by the wallet SDK.
check_sdk_parses_addresses() {
    skip_without_client_stack && return
    local out="$EV/sdk_parse_addresses" unparsed
    if client_run "$out" wallet_check.mjs parse-address \
        --unshielded "$(tk_address "$SENDER_SEED" --unshielded)" \
        --shielded "$(tk_address "$SENDER_SEED" --shielded)" \
        --dust "$(tk_address "$SENDER_SEED" --dust)"; then
        pass "unshielded, shielded and DUST addresses printed by $TK_IMAGE all parse"
        return
    fi
    unparsed=$(jq -r '[.results[] | select(.parsed | not) | "\(.kind) (\(.length) chars): \(.error)"] | join("; ")' "$out.json" 2>/dev/null)
    fail "${unparsed:-$(client_error "$out")}"
}

check_toolkit_proves_at_proof_server() {
    local problem
    problem=$(send_night toolkit_remote_proving --source-seed "$SENDER_SEED" --unshielded-amount 1 \
        --destination-address "$SPARE_ADDRESS" --proof-server "$PROOF_SERVER_URL") \
        && pass "proved at the $PROOF_SERVER_TAG proof server and accepted" || fail "$problem"
}

# --- The matrix -------------------------------------------------------------------------
# Wallet funding, DUST registration and the Midnight.js dApp calls differ between the two
# groups, so each group runs those itself, around these.

# Group 2 files each set of checks under its own runbook bullet; group 7 files them all
# under one, the regression matrix.
feature_bullet() { bullet "${REGRESSION_BULLET:-$1}"; }
run_transfer_checks() {
    feature_bullet "Transfers"
    check transfer-unshielded         "an unshielded NIGHT transfer, sender to receiver"                    check_transfer_unshielded
    check transfer-shielded           "a shielded NIGHT transfer, sender to receiver"                       check_transfer_shielded
    check transfer-mixed              "one transaction with a shielded and an unshielded output"            check_transfer_mixed
    check transfer-two-destinations   "one unshielded transfer to two destinations"                         check_transfer_two_destinations
    check transfer-from-second-wallet "an unshielded NIGHT transfer back, receiver to sender"               check_transfer_from_second_wallet
}

run_contract_checks() {
    feature_bullet "Contracts: deploy, call, read state, rotate the maintenance authority"
    check contract-store-a            "a contract with a one-member committee: deploy, store, check"        check_contract_store_a
    check contract-store-b-committee  "a contract with a two-member committee: deploy, store, check"        check_contract_store_b_committee
    check contract-authority-rotated  "the two-member committee is replaced, signed by a current member"    check_contract_authority_rotated
}

run_dapp_checks() {
    feature_bullet "dApps: deploy and call a few real ones, including one that holds funds"
    check dapp-counter                "counter: deploy, increment twice"                                    check_dapp_counter
    check dapp-bboard                 "bboard: deploy, post a message"                                      check_dapp_bboard
    check dapp-micro-dao              "micro-dao: deploy, its entry points are on chain"                    check_dapp_micro_dao
    check dapp-vault-mints-and-pays   "vault: deploy, mint its own token, pay some out"                     check_dapp_vault_mints_and_pays
    check game-draw-played            "a rock-paper-scissors game played to the end: stakes in, a draw, refunds" check_game_draw_played
}

run_version_checks() {
    feature_bullet "Ledger parameters and versions as every node and tool reports them"
    check ledger-version              "the node reports ledger $EXPECTED_LEDGER_VERSION"                    check_ledger_version
    check runtime-spec                "the runtime spec is $EXPECTED_SPEC"                                  check_runtime_spec
    check toolkit-version             "the toolkit is node $EXPECTED_NODE_VERSION with compactc $EXPECTED_COMPACTC" check_toolkit_version
    check chain-healthy               "blocks are produced and finalized over 60 seconds"                   check_chain_healthy 7
}

run_indexer_checks() {
    feature_bullet "The indexer: synced, contracts and transactions queryable"
    check indexer-at-tip              "the indexer follows the finalized tip"                               check_indexer_at_tip
    check indexer-has-contracts       "every contract deployed above is in the indexer"                     check_indexer_has_contracts
    check indexer-has-transactions    "every transaction sent above is in a block, found by hash"           check_indexer_has_transactions
}

run_client_stack_checks() {
    feature_bullet "The client stack: wallet SDK, Midnight.js, the proof server"
    check client-stack-ready          "proof servers up, the indexer serves the SDK, client packages installed" check_client_stack_ready
    check clients-read-chain-era      "the wallet SDK and Midnight.js read the chain head as ledger $LEDGER" check_clients_read_chain_era
    check sdk-wallet-syncs            "a wallet syncs through the wallet SDK and shows NIGHT and DUST"      check_sdk_wallet_syncs
    check sdk-transfer-unshielded     "an unshielded transfer through the wallet SDK, proved at the $PROOF_SERVER_TAG proof server" check_sdk_transfer_unshielded
    check sdk-transfer-shielded       "a shielded transfer through the wallet SDK"                          check_sdk_transfer_shielded
    check sdk-history                 "the wallet SDK's transaction history holds these transfers"          check_sdk_history
    check sdk-parses-addresses        "the wallet SDK parses the addresses the toolkit prints"              check_sdk_parses_addresses
    check toolkit-proves-at-proof-server "a toolkit transfer proved at the proof server, not in-process"     check_toolkit_proves_at_proof_server
}
