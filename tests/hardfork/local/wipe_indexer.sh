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

# Group 10 on local-env, re-sync: the indexer is wiped after the fork. It must replay the
# chain from genesis through the fork block, and a wallet must see the same balance through
# it. Its database and its message store both go, so nothing resumes from a state cached
# after the fork.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/local_env.sh"

evidence_dir g10_wipe_indexer
recall_state
use_wallets genesis
require_chain_on new
require_indexer
require_clients
CHECKOUT="${LOCAL_ENV_CHECKOUT:-$NEW_ROOT}"
TARGET_HEIGHT=$(finalized_height)
NIGHT_BEFORE_WIPE=""

group_start G10 "Failure and recovery drills on local-env: a wiped indexer re-syncs"

sender_night_through_indexer() {  # <evidence name>
    client_run "$EV/$1" wallet_check.mjs sync || return 1
    jq -r --arg night "$NIGHT_TOKEN_TYPE" '.wallet.unshielded.balances[$night] // "0"' "$EV/$1.json"
}

wipe_and_restart_indexer() {
    ( load_compose_env "$CHECKOUT" "$(new_node_image)" "$(new_toolkit_image)" > /dev/null
      docker rm -f chain-indexer wallet-indexer indexer-api postgres-indexer nats > /dev/null 2>&1 || true
      docker volume rm local-env_postgres-indexer-data local-env_nats-data > /dev/null 2>&1 || true
      indexer_compose "$CHECKOUT" up -d postgres-indexer nats chain-indexer wallet-indexer indexer-api ) > "$EV/recreate_indexer.log" 2>&1
}

# Prints the height it reached; fails when the chain indexer died or did not get there.
wait_for_indexer_replay() {  # <seconds>
    local waited=0 height=0
    while [ "$waited" -lt "$1" ]; do
        if ! docker inspect -f '{{.State.Running}}' chain-indexer 2> /dev/null | grep -q true; then
            echo "the chain indexer stopped at #$height: $(docker logs chain-indexer 2>&1 | grep -a 'ERROR' | tail -1 | cut -c1-300)"
            return 1
        fi
        height=$(indexer_height)
        if [ "${height:-0}" -ge "$TARGET_HEIGHT" ]; then echo "$height"; return 0; fi
        sleep 5; waited=$(( waited + 5 ))
    done
    echo "the indexer is at #$height after $(( $1 / 60 )) minutes, the target is #$TARGET_HEIGHT"
    return 1
}

check_wiped_indexer_resyncs() {
    local started reached protocol_version
    indexer_catch_up 300 > /dev/null || { fail "the indexer is not at the tip before the wipe"; return; }
    proof_server_up new > /dev/null 2>&1 || true
    NIGHT_BEFORE_WIPE=$(sender_night_through_indexer wallet_before_wipe) || { NIGHT_BEFORE_WIPE=""; fail "the sender's wallet does not sync before the wipe: $(client_error "$EV/wallet_before_wipe")"; return; }
    wipe_and_restart_indexer || { fail "recreating the indexer: $(last_line_of "$EV/recreate_indexer.log")"; return; }
    started=$(date +%s)
    reached=$(wait_for_indexer_replay 900) || { fail "$reached"; return; }
    protocol_version=$(gql_query '{ block { protocolVersion } }' | jq -r .data.block.protocolVersion)
    [ "$protocol_version" = "$(spec_version)" ] || { fail "the rebuilt indexer's tip says protocolVersion $protocol_version, the runtime spec is $(spec_version)"; return; }
    pass "the wiped indexer replayed genesis to #$reached in $(( $(date +%s) - started ))s, across fork block #${FORK_HEIGHT:-?}"
}

check_wallet_same_on_rebuilt_indexer() {
    local night_now
    [ -n "$NIGHT_BEFORE_WIPE" ] || { skip "the wallet's balance before the wipe is unknown (G10-wiped-indexer-resyncs)"; return; }
    night_now=$(sender_night_through_indexer wallet_after_wipe) || { fail "$(client_error "$EV/wallet_after_wipe")"; return; }
    if [ "$night_now" = "$NIGHT_BEFORE_WIPE" ]; then pass "the sender's wallet shows NIGHT $night_now through the rebuilt indexer, as before the wipe"
    else fail "the sender's wallet shows NIGHT $night_now through the rebuilt indexer, $NIGHT_BEFORE_WIPE before the wipe"; fi
}

bullet "Re-sync: a wiped validator and a wiped indexer sync from genesis through the fork"
check wiped-indexer-resyncs           "a wiped indexer replays the chain from genesis through the fork"          check_wiped_indexer_resyncs
check wallet-same-on-rebuilt-indexer  "a wallet shows the same balance through the rebuilt indexer"              check_wallet_same_on_rebuilt_indexer

group_finish
