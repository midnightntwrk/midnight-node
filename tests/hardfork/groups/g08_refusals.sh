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

# Group 8: outdated and invalid inputs are refused.
#
# Does each case end in a clear error, with no lost funds, no hang and no damaged chain?
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

start_after_fork g8
require_clients

group_start G8 "Outdated and invalid inputs are refused, on $NETWORK_NAME (ledger $NEW_LEDGER, fork block #$FORK_HEIGHT)"

# The transaction is handed to the node as a bare extrinsic, so no client can refuse it first.
# The node must fail to decode it, and the holder's funds must be untouched.
check_saved_old_transaction_refused() {
    local saved="$STATE_DIR/unsent_old_transaction.extrinsic" extrinsic validation submission holder_night cannot_decode_verdict
    # An invalid transaction (01), custom (00 07), with the error's number.
    cannot_decode_verdict=$(printf '0x010007%02x' "$NODE_ERROR_CANNOT_DECODE")
    [ -s "$saved" ] || { skip "no old-ledger transaction was saved (G2-unsent-transaction-saved)"; return; }
    extrinsic=$(cat "$saved")
    validation=$(validate_extrinsic "$extrinsic"); submission=$(submit_extrinsic "$extrinsic")
    printf 'validate: %s\nsubmit: %s\n' "$validation" "$submission" > "$EV/saved_old_transaction.txt"
    holder_night=$(wallet_night "$STATE_DIR/holder_wallet_after_fork.json" 2> /dev/null)
    if ! night_kept "${HOLDER_NIGHT_BEFORE_FORK:-}" "$holder_night"; then
        fail "the holder's NIGHT moved since group 2 (${HOLDER_NIGHT_BEFORE_FORK:-unrecorded} -> ${holder_night:-unreadable}): the transaction's inputs may be spent, so a refusal proves nothing"
    elif [ "$validation" != "$cannot_decode_verdict" ]; then
        fail "validate_transaction says $validation, expected $cannot_decode_verdict (Custom error $NODE_ERROR_CANNOT_DECODE: it cannot be decoded); submitting gave '$submission'"
    elif [ "$submission" = "1010 Invalid Transaction: Custom error: $NODE_ERROR_CANNOT_DECODE" ]; then
        pass "refused while decoding: $submission; the holder still has $holder_night NIGHT (unshielded, shielded)"
    elif [[ "$submission" == "1012 Transaction is temporarily banned"* ]]; then
        pass "banned by the pool after an earlier refusal, and validation still says it cannot be decoded; the holder still has $holder_night NIGHT (unshielded, shielded)"
    else
        fail "submitting gave '$submission', expected 1010 Custom error $NODE_ERROR_CANNOT_DECODE"
    fi
}

# One byte of a valid new-ledger transaction is flipped. The node must refuse it in
# validation, with an error other than the decoding error.
check_malformed_transaction_refused() {
    local tx_file="$EV/valid_transaction.mn" intact_verdict submission node_error
    tk_chain generate-txs --dest-file "$tx_file" single-tx --source-seed "$SENDER_SEED" --unshielded-amount 1 \
        --destination-address "$SPARE_ADDRESS" > "$EV/valid_transaction.log" 2>&1 && [ -s "$tx_file" ] \
        || { fail "$(last_line_of "$EV/valid_transaction.log")"; return; }
    intact_verdict=$(validate_extrinsic "$(bare_extrinsic "$tx_file")")
    [[ "$intact_verdict" == 0x00* ]] || { fail "the intact transaction is not valid either: $intact_verdict"; return; }
    submission=$(submit_extrinsic "$(bare_extrinsic "$tx_file" --corrupt)")
    printf 'intact: %s\ncorrupted: %s\n' "$intact_verdict" "$submission" > "$EV/malformed_transaction.txt"
    node_error=$(grep -oE 'Custom error: [0-9]+$' <<< "$submission" | grep -oE '[0-9]+$')
    if [[ "$submission" == accepted* ]]; then fail "the corrupted transaction was accepted: $submission"
    elif [[ "$submission" != "1010 "* ]]; then fail "${submission:-the node did not answer}"
    elif [ -z "$node_error" ] || [ "$node_error" = "$NODE_ERROR_CANNOT_DECODE" ]; then fail "$submission: expected a validation error, not the decoding error $NODE_ERROR_CANNOT_DECODE"
    else pass "the intact copy is valid; with one byte flipped it is refused in validation: $submission"; fi
}

# The old proof server keeps answering after the fork, but its proofs are for the old ledger.
check_old_proof_server_proof_refused() {
    local out="$EV/proof_from_old_proof_server" node_error
    proof_server_up old > "$EV/proof_server_old.log" 2>&1 || { fail "the $PROOF_SERVER_OLD_TAG proof server could not be started"; return; }
    if CLIENT_PROOF_SERVER_NEW="$PROOF_SERVER_OLD_URL" client_run "$out" wallet_check.mjs send --kind unshielded --amount 1; then
        fail "a transfer proved at the $PROOF_SERVER_OLD_TAG proof server was accepted on ledger $NEW_LEDGER"
        return
    fi
    # Only the node refusing the proof counts; a failure before that proves nothing.
    node_error=$(client_custom_error "$out")
    if [ -z "$node_error" ]; then fail "it failed before the node judged the proof: $(client_error "$out")"
    elif [ -n "${NODE_ERRORS_FOR_OLD_PROOF[$node_error]:-}" ]; then pass "the node refused it with Custom error $node_error, ${NODE_ERRORS_FOR_OLD_PROOF[$node_error]}"
    else fail "the node refused it with Custom error $node_error, expected one of: ${!NODE_ERRORS_FOR_OLD_PROOF[*]} (${NODE_ERRORS_FOR_OLD_PROOF[*]})"; fi
}

# An application that never upgraded: the wallet SDK release from before the fork. It must
# not transact, and how it fails is what its users meet.
check_old_wallet_cannot_transact() {
    local out="$EV/old_wallet_send" error node_error
    if old_wallet_send "$out"; then
        fail "wallet SDK $(old_wallet_sdk_version) transacted on ledger $NEW_LEDGER: transaction $(jq -r .txId "$out.json")"
        return
    fi
    error=$(client_error "$out"); node_error=$(client_custom_error "$out")
    if [[ "$error" == *MN_SYNC_TIMEOUT_MS* || "$error" == *MN_TIMEOUT_SECS* ]]; then
        warn "wallet SDK $(old_wallet_sdk_version) hangs: no result after ${OLD_WALLET_LIMIT_SECS}s and no error for its user; it logs: $(grep -m1 -o "message: '[^']*'" "$out.err" || echo nothing)"
    else
        pass "wallet SDK $(old_wallet_sdk_version) cannot transact and says so: $error${node_error:+ (Custom error $node_error)}"
    fi
}

check_chain_unharmed() {
    local node height_before problem unreachable=0
    for node in "${NODES[@]}"; do
        [ -n "$(best_height_at "${node#*=}")" ] || unreachable=$(( unreachable + 1 ))
    done
    [ "$unreachable" = 0 ] || { fail "$unreachable node(s) stopped answering"; return; }
    height_before=$(node_height)
    wait_blocks 2 60 > /dev/null || { fail "block production stopped at #$height_before"; return; }
    problem=$(send_night transfer_after_refusals --source-seed "$HOLDER_SEED" --unshielded-amount 2 --destination-address "$SPARE_ADDRESS") \
        || { fail "a transfer from the holder after the refusals: $problem"; return; }
    pass "every node answers, blocks went #$height_before -> #$(node_height), and the holder's own new-ledger transfer landed"
}

bullet "The old-ledger transaction saved in group 2"
check saved-old-transaction-refused     "the node refuses the saved old-ledger transaction; the holder's funds are untouched" check_saved_old_transaction_refused

bullet "A malformed new-ledger transaction"
check malformed-transaction-refused     "the node refuses a valid transaction with one byte flipped"                       check_malformed_transaction_refused

bullet "A proof from the old-ledger proof server"
check old-proof-server-proof-refused    "a transfer proved at the $PROOF_SERVER_OLD_TAG proof server is refused by the node" check_old_proof_server_proof_refused

bullet "An un-upgraded wallet trying to transact"
check old-wallet-cannot-transact        "the wallet SDK release from before the fork cannot transact"                      check_old_wallet_cannot_transact

bullet "Never a damaged chain"
check chain-unharmed                    "after the refusals every node is up, blocks are produced and a transfer lands"    check_chain_unharmed

group_finish
