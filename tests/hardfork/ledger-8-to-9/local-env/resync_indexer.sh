#!/usr/bin/env bash
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

# Table HF12: a wiped indexer replays genesis -> fork -> tip and a wallet sees the same
# NIGHT through it. Postgres and the NATS bucket both go, so nothing resumes from a cached
# post-fork state. Then the released indexer against this node (INDEXER_PAIRING_CHECK=0
# skips it): it crosses the fork only when built against a matching node release.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/lib_local.sh"

EV="$EVIDENCE_DIR/indexer_resync"; mkdir -p "$EV"
require_era l9
state_load "$STATE_DIR/env.env"; state_load "$FORK_STATE"
CK="${LOCAL_ENV_CHECKOUT:-$L9_ROOT}"
TIP=$(finalized_height); SPEC=$(spec_version)
indexer_resolve || true
clients_ready > "$EV/clients_ready.log" 2>&1 || true

recreate() {  # <indexer tag>
    ( INDEXER_TAG="$1" load_compose_env "$CK" "$(l9_node_image)" "$(l9_toolkit_image)" >/dev/null
      docker rm -f chain-indexer wallet-indexer indexer-api postgres-indexer nats >/dev/null 2>&1 || true
      docker volume rm local-env_postgres-indexer-data local-env_nats-data >/dev/null 2>&1 || true
      indexer_compose "$CK" up -d postgres-indexer nats chain-indexer wallet-indexer indexer-api ) > "$EV/recreate_$1.log" 2>&1
}
replay() {  # <max-seconds> -> REPLAY=CROSSED|DIED|STUCK, REPLAY_H, REPLAY_ERR
    local waited=0; REPLAY=STUCK; REPLAY_ERR=""
    while [ "$waited" -lt "$1" ]; do
        if ! docker inspect -f '{{.State.Running}}' chain-indexer 2>/dev/null | grep -q true; then
            REPLAY=DIED; REPLAY_ERR=$(docker logs chain-indexer 2>&1 | grep -a 'ERROR' | tail -1 | cut -c1-300); return
        fi
        REPLAY_H=$(indexer_height); [ "${REPLAY_H:-0}" -ge "$TIP" ] && { REPLAY=CROSSED; return; }
        sleep 5; waited=$((waited + 5))
    done
}
night_of() { jq -r --arg t "$NIGHT_TOKEN_TYPE" '.wallet.unshielded.balances[$t] // "0"' "$1"; }

t_table HF12 "A wiped indexer ($INDEXER_TAG) re-syncs genesis -> fork -> tip (finalized #$TIP); pairing of the released $INDEXER_RELEASED_TAG"

NIGHT_BEFORE=""
c_snapshot() {
    indexer_catch_up 300 >/dev/null || { t_fail "the live indexer is not at the tip"; return; }
    proof_server_up l9 >/dev/null 2>&1 || true
    client_run "$EV/wallet_before" wallet_check.mjs sync || { t_fail "$(client_error "$EV/wallet_before")"; return; }
    NIGHT_BEFORE=$(night_of "$EV/wallet_before.json"); t_pass "seed 1 NIGHT $NIGHT_BEFORE at indexer #$(indexer_height)"
}
c_replay() {
    recreate "$INDEXER_TAG" || { t_fail "recreate: $(last_line_of "$EV/recreate_$INDEXER_TAG.log")"; return; }
    local t0; t0=$(date +%s); replay 900
    case "$REPLAY" in
        DIED) t_fail "chain-indexer stopped at #$REPLAY_H: $REPLAY_ERR" ;;
        STUCK) t_fail "at #$REPLAY_H after 15 min (target $TIP)" ;;
        *) local pv; pv=$(gql_query '{ block { protocolVersion } }' | jq -r .data.block.protocolVersion)
           [ "$pv" = "$SPEC" ] && t_pass "replayed to #$REPLAY_H in $(( $(date +%s) - t0 )) s across fork block #${FORK_HEIGHT:-?}; protocolVersion $pv" \
               || t_fail "tip protocolVersion $pv, node spec $SPEC" ;;
    esac
}
c_wallet() {
    [ -n "$NIGHT_BEFORE" ] || { t_skip "no baseline (HF12-1)"; return; }
    client_run "$EV/wallet_after" wallet_check.mjs sync || { t_fail "$(client_error "$EV/wallet_after")"; return; }
    local n; n=$(night_of "$EV/wallet_after.json")
    [ "$n" = "$NIGHT_BEFORE" ] && t_pass "NIGHT $n, as before the wipe" || t_fail "NIGHT $n through the rebuilt indexer, $NIGHT_BEFORE before"
}
c_pairing() {
    [ "${INDEXER_PAIRING_CHECK:-1}" = 1 ] && [ "$INDEXER_RELEASED_TAG" != "$INDEXER_TAG" ] || { t_skip "INDEXER_PAIRING_CHECK=0, or the pinned tag is the released one"; return; }
    require_image "$IMAGE_REGISTRY/chain-indexer:$INDEXER_RELEASED_TAG" || { t_fail "released image missing"; return; }
    recreate "$INDEXER_RELEASED_TAG"; replay 600
    local verdict="$REPLAY at #${REPLAY_H:-0}: ${REPLAY_ERR:-reached the tip}"
    recreate "$INDEXER_TAG"; replay 900
    [ "$REPLAY" = CROSSED ] || { t_fail "the pinned indexer did not come back ($REPLAY at #$REPLAY_H); released: $verdict"; return; }
    case "$verdict" in
        CROSSED*) t_pass "the released $INDEXER_RELEASED_TAG crosses the fork against this node" ;;
        *) t_warn "the released $INDEXER_RELEASED_TAG does not work with this node (a release built for it should): $verdict" ;;
    esac
}
t_check HF12-1 "HF-12" "wallet snapshot through the live indexer" c_snapshot
t_check HF12-2 "HF-12" "the wiped indexer replays genesis -> fork -> tip" c_replay
t_check HF12-3 "HF-12" "a wallet sees the same NIGHT through the rebuilt indexer" c_wallet
t_check HF12-4 "HF-12" "pairing: the released indexer $INDEXER_RELEASED_TAG bootstrapped against this node" c_pairing
t_finish
