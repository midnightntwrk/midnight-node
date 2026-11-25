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

# The operators' part on local-env: the hard fork as a network does it.
#
#   1. prepare the new release
#   2. what this fork's release needs before the first wave (forks/<fork>/rollout_local.sh)
#   3. wave 1: validator 1 moves to the new binary        -> group 3 smoke
#      a validator of the next wave is killed             -> group 10
#   4. wave 2: validators 2 to 5 move to the new binary   -> group 3 smoke; the killed one rejoins
#   5. the state is recorded                              -> group 4, record
#   6. the crosser's wallet starts transacting            -> group 4, crosser
#      a validator is stopped                             -> group 10
#   7. governance submits the runtime upgrade: the fork
#   8. the stopped validator comes back                   -> group 10
#
# An operator step that fails stops the rollout; failed smoke checks are recorded and it
# goes on. The drop-outs are checks of group 10 and are recorded there. LAGGARD=<validator>
# keeps that validator on the old binary through the fork instead of stopping one (2, 3 or 4).
set -e
source "$(dirname "${BASH_SOURCE[0]}")/local_env.sh"

WAVES=("1" "2 3 4 5")
KILLED_DURING_ROLLOUT=2      # a validator of wave 2, killed after wave 1
OFFLINE_AT_FORK=4            # stopped before the runtime upgrade, started after it
LAGGARD="${LAGGARD:-}"
[ -z "$LAGGARD" ] || OFFLINE_AT_FORK=""
BLOCKS_OFFLINE_AFTER_FORK=10
SECONDS_OF_TRAFFIC_BEFORE_UPGRADE=90
LOGS="${RUN_DIR:-$RUNS_DIR}"
FORK_BLOCK_SCRIPT="$SUITE_DIR/groups/g04_fork_block_integrity.sh"

evidence_dir rollout
recall_state
use_wallets genesis
require_group2_record
[ "$(spec_version)" = "$SPEC_BEFORE_FORK" ] || cannot_run "the chain is on spec $(spec_version), not $SPEC_BEFORE_FORK: has it forked already?"

group_start G10 "Failure and recovery drills on local-env: validators dropping out during the rollout"

# An operator step: it must work, or the rollout stops here. The step prints what it did, or
# what went wrong. It runs in this shell, so what it sets (a process ID, a height) stays.
operator_step() {  # <what> <function> [arguments...]
    local what=$1 report="$EV/operator_step.txt"
    shift
    echo ""
    echo ">>> operator: $what"
    if "$@" > "$report"; then sed 's/^/    /' "$report"
    else cannot_run "$what: $(cat "$report")"; fi
}

# --- 1. The new release ---------------------------------------------------------------------
prepare_new_release() {
    local ref="${NEW_REF:-}"
    if [ -z "$ref" ] && [ -n "${NEW_PR:-}" ]; then
        git -C "$REPO_ROOT" fetch -q origin "pull/$NEW_PR/head:refs/remotes/origin/pr-$NEW_PR" && ref="origin/pr-$NEW_PR"
    fi
    [ -n "$ref" ] || { echo "set NEW_REF (a tag or a commit) or NEW_PR"; return 1; }
    ensure_worktree "$NEW_ROOT" "$ref" || { echo "cannot check out $ref at $NEW_ROOT"; return 1; }
    git -C "$NEW_ROOT" checkout -q -- local-environment 2> /dev/null || true
    require_image "$(new_node_image)" && require_image "$(new_toolkit_image)" || { echo "the images of $ref are missing"; return 1; }
    prepare_new_checkout "$NEW_ROOT" || { echo "the fork's changes to the new checkout failed"; return 1; }
    # Services recreated from the new checkout must connect with the running stack's credentials.
    cp "$(local_env_dir "$OLD_ROOT")"/localenv_*.password "$(local_env_dir "$NEW_ROOT")/" || { echo "no compose secrets in the old checkout"; return 1; }
    rm -rf "$(compose_dir "$NEW_ROOT")/runtime-values"
    cp -R "$(compose_dir "$OLD_ROOT")/runtime-values" "$(compose_dir "$NEW_ROOT")/" 2> /dev/null || true
    ( load_compose_env "$NEW_ROOT" "$(new_node_image)" "$(new_toolkit_image)" && npm ci --no-audit --no-fund --loglevel=error ) > "$EV/npm_ci.log" 2>&1 \
        || { echo "npm ci in the new release's local-environment failed ($EV/npm_ci.log)"; return 1; }
    remember LOCAL_ENV_CHECKOUT "$NEW_ROOT"
    echo "$(git -C "$NEW_ROOT" log -1 --format='%h %s' | cut -c1-80); node $(new_node_image)"
}

# --- 3 and 4. A binary wave -------------------------------------------------------------------
UPGRADED_VALIDATORS=0
swap_binaries() {  # <validator>...
    local validator version attempt containers=()
    for validator in "$@"; do containers+=("$(validator_name "$validator")"); done
    compose_new_release config -q > "$EV/compose_config.log" 2>&1 || { echo "the new release's compose project does not load; nothing was stopped"; return 1; }
    docker stop "${containers[@]}" > /dev/null
    compose_new_release up -d --no-deps "${containers[@]}" > "$EV/compose_up_${containers[0]}.log" 2>&1 || { echo "compose up ${containers[*]} failed"; return 1; }
    for validator in "$@"; do
        for attempt in $(seq 1 24); do
            version=$(node_version_at "$(node_url "$(validator_name "$validator")")")
            [ -n "$version" ] && break
            sleep 5
        done
        [[ "$version" == "$NEW_EXPECTED_NODE_VERSION"* ]] || { echo "$(validator_name "$validator") reports '$version', expected $NEW_EXPECTED_NODE_VERSION"; return 1; }
    done
    echo "${containers[*]} on $NEW_EXPECTED_NODE_VERSION, each on its existing database"
}

smoke_after_wave() {  # <wave number>
    local log="$LOGS/g3-wave$1.log"
    local exit_code=0
    EXPECTED_UPGRADED_NODES=$UPGRADED_VALIDATORS "$SUITE_DIR/groups/g03_wave_smoke.sh" "$1" > "$log" 2>&1 || exit_code=$?
    # Failed checks are recorded in group 3 and the rollout goes on; a smoke that could not run stops it.
    [ "$exit_code" -lt 2 ] || { echo "the smoke after wave $1 could not run ($log)"; return 1; }
    log_group_counts "$log"
}

# --- Group 10: validators dropping out ------------------------------------------------------------
# GRANDPA finalizes with more than two thirds of its voters. local-env elects three, so with
# one of them down finality waits; block production must not.
check_validator_killed_during_wave() {
    local victim health
    victim=$(validator_name "$KILLED_DURING_ROLLOUT")
    docker kill "$victim" > /dev/null 2>&1 || { fail "docker kill $victim"; return; }
    if health=$(chain_health 60); then
        pass "$victim, still on the old binary, was killed; the others keep producing and finalizing: $health"
    elif finality_spares_a_voter; then
        fail "with $victim down: $health"
    elif wait_blocks 3 60 > /dev/null; then
        pass "$victim, still on the old binary, was killed; the others keep producing blocks (best #$(node_height)). Finality waits for it: GRANDPA needs $(finality_voters) voters here"
    else
        fail "with $victim down, block production stopped: $health"
    fi
}

check_killed_validator_rejoins() {
    local victim caught_up finality finalized
    victim=$(validator_name "$KILLED_DURING_ROLLOUT")
    caught_up=$(wait_until_caught_up "$victim") || { fail "$victim $caught_up"; return; }
    finality=$(wait_finality_progress 2 180) || { fail "$victim is back, $caught_up, but $finality"; return; }
    finalized=$(finalized_height)
    has_same_block_as_reference "$victim" "$finalized" || { fail "$victim has another block at finalized #$finalized"; return; }
    pass "$victim came back on the new binary from the database it was killed with: $caught_up; $finality"
}

check_validator_offline_at_fork() {
    local victim finality
    victim=$(validator_name "$OFFLINE_AT_FORK")
    if finality=$(wait_finality_progress 3 120); then
        [ "$(finalized_height)" -ge "$FORK_HEIGHT" ] || { fail "finality is at #$(finalized_height), below fork block #$FORK_HEIGHT"; return; }
        pass "$victim was offline from #$WENT_OFFLINE_AT; the others enacted the upgrade and finalized past fork block #$FORK_HEIGHT: $finality"
    elif finality_spares_a_voter; then
        fail "with $victim offline: $finality"
    elif wait_blocks 3 60 > /dev/null && [ "$(node_height)" -gt "$FORK_HEIGHT" ]; then
        pass "$victim was offline from #$WENT_OFFLINE_AT; the others enacted the upgrade and produce past fork block #$FORK_HEIGHT (best #$(node_height)). Finality waits for it: GRANDPA needs $(finality_voters) voters here"
    else
        fail "with $victim offline, block production stopped at #$(node_height) (fork block #$FORK_HEIGHT)"
    fi
}

# It was down at the fork block, so it must import that block itself, migration included.
check_offline_validator_catches_up() {
    local victim started_at caught_up migration finality
    victim=$(validator_name "$OFFLINE_AT_FORK")
    wait_blocks "$BLOCKS_OFFLINE_AFTER_FORK" > /dev/null || true
    started_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
    docker start "$victim" > /dev/null || { fail "docker start $victim"; return; }
    caught_up=$(wait_until_caught_up "$victim") || { fail "$victim $caught_up"; return; }
    has_same_block_as_reference "$victim" "$FORK_HEIGHT" || { fail "$victim has another block at fork block #$FORK_HEIGHT"; return; }
    migration=$(docker logs --since "$started_at" "$victim" 2>&1 | grep -oE 'translation complete in [0-9]+ step\(s\), [^,]+, [0-9]+ps synthetic cost' | tail -1)
    [ -n "$migration" ] || { fail "$victim caught up, $caught_up, but logged no ledger migration since its restart"; return; }
    finality=$(wait_finality_progress 2 180) && [ "$(finalized_height)" -ge "$FORK_HEIGHT" ] \
        || { fail "$victim caught up, $caught_up, but finality is at #$(finalized_height) (fork block #$FORK_HEIGHT): $finality"; return; }
    pass "$victim, down from #$WENT_OFFLINE_AT, imported fork block #$FORK_HEIGHT itself ($migration) and caught up: $caught_up; $finality"
}

check_laggard_stops_at_fork() {
    local victim best status finality
    victim=$(validator_name "$LAGGARD")
    finality=$(wait_finality_progress 3 120) || { fail "the upgraded validators: $finality"; return; }
    sleep 30
    status=$(docker inspect -f '{{.State.Status}}' "$victim" 2> /dev/null); best=$(best_height_at "$(node_url "$victim")")
    docker logs --tail 400 "$victim" > "$EV/laggard.log" 2>&1 || true
    if [ -z "$best" ] || [ "$best" -lt "$FORK_HEIGHT" ]; then
        pass "$victim ($status), left on the old binary, stopped at #${best:-unreachable}, below fork block #$FORK_HEIGHT, while the others finalize ($finality); it logs: $(grep -m1 -aiE 'error|fail|unsupported|not found' "$EV/laggard.log" | cut -c1-140)"
    elif has_same_block_as_reference "$victim" "$FORK_HEIGHT"; then
        warn "$victim imported fork block #$FORK_HEIGHT and follows the chain on the old binary: the binary rollout may not be needed before the runtime upgrade"
    else
        fail "$victim follows another chain from fork block #$FORK_HEIGHT (best #$best)"
    fi
}

check_laggard_recovers_once_upgraded() {
    local victim caught_up version finalized
    victim=$(validator_name "$LAGGARD")
    compose_new_release up -d --no-deps "$victim" > "$EV/compose_up_laggard.log" 2>&1 || { fail "compose up $victim"; return; }
    caught_up=$(wait_until_caught_up "$victim") || { fail "$victim $caught_up"; return; }
    version=$(node_version_at "$(node_url "$victim")"); finalized=$(finalized_height)
    [[ "$version" == "$NEW_EXPECTED_NODE_VERSION"* ]] || { fail "$victim runs '$version'"; return; }
    has_same_block_as_reference "$victim" "$finalized" || { fail "$victim has another block at finalized #$finalized"; return; }
    pass "$victim moved to $version after the fork and caught up through the fork block: $caught_up"
}

# --- 5 to 7. The runtime upgrade ----------------------------------------------------------------
record_state_before_upgrade() {
    local log="$LOGS/g4-record.log"
    "$FORK_BLOCK_SCRIPT" record > "$log" 2>&1 || { echo "the record before the upgrade failed: $(log_group_counts "$log") ($log)"; return 1; }
    log_group_counts "$log"
}

# The runtime governance enacts: the release asset group 1 verified, or, for a ref that is
# not a release, the runtime inside its node image.
RUNTIME_IN_LOCAL_ENV="$(local_env_dir "$NEW_ROOT")/artifacts/test/midnight_node_runtime.compact.compressed.wasm"
place_runtime_wasm() {
    mkdir -p "$(dirname "$RUNTIME_IN_LOCAL_ENV")"
    if [ -s "$STATE_DIR/runtime.wasm" ]; then
        cp "$STATE_DIR/runtime.wasm" "$RUNTIME_IN_LOCAL_ENV"
        echo "the release's runtime wasm, verified in group 1 (blake2-256 ${RUNTIME_WASM_BLAKE2:-?})"
    else
        docker run --rm --entrypoint cat "$(new_node_image)" "/artifacts-$ARCH/midnight_node_runtime.compact.compressed.wasm" > "$RUNTIME_IN_LOCAL_ENV" 2> /dev/null
        [ -s "$RUNTIME_IN_LOCAL_ENV" ] || { echo "no runtime wasm: group 1 verified none and the node image has none"; return 1; }
        echo "the runtime wasm from the node image $(new_node_image) (group 1 verified no release asset)"
    fi
}

CROSSER_PID=""
start_crosser() {
    "$FORK_BLOCK_SCRIPT" crosser > "$LOGS/g4-crosser.log" 2>&1 &
    CROSSER_PID=$!
    sleep "$SECONDS_OF_TRAFFIC_BEFORE_UPGRADE"
    kill -0 "$CROSSER_PID" 2> /dev/null || { echo "the crosser's wallet stopped before the upgrade ($LOGS/g4-crosser.log)"; return 1; }
    echo "the crosser's wallet has been transacting for ${SECONDS_OF_TRAFFIC_BEFORE_UPGRADE}s ($LOGS/g4-crosser.log)"
}

WENT_OFFLINE_AT=""
stop_validator_before_upgrade() {
    WENT_OFFLINE_AT=$(node_height)
    docker stop "$(validator_name "$OFFLINE_AT_FORK")" > /dev/null || return 1
}

# The upgrade tool can fail to decode the events of the upgrade block after the upgrade was
# applied, so the runtime spec is the verdict.
submit_runtime_upgrade() {
    local council technical_committee spec attempt
    IFS=, read -r -a council <<< "$GOV_COUNCIL_URIS"
    IFS=, read -r -a technical_committee <<< "$GOV_TC_URIS"
    ( load_compose_env "$NEW_ROOT" "$(new_node_image)" "$(new_toolkit_image)" > /dev/null && npm run governance-runtime-upgrade:local-env -- \
        --wasm test/midnight_node_runtime.compact.compressed.wasm --rpc-url ws://localhost:9933 \
        --council-uris "${council[@]}" --technical-uris "${technical_committee[@]}" --executor-uri "$GOV_EXECUTOR_URI" --skip-run ) \
        > "$EV/runtime_upgrade.log" 2>&1 || true
    for attempt in $(seq 1 36); do
        spec=$(spec_version)
        [ "$spec" != "$SPEC_BEFORE_FORK" ] && break
        sleep 5
    done
    [ "$spec" = "$NEW_EXPECTED_SPEC" ] || { echo "the runtime spec is $spec, expected $NEW_EXPECTED_SPEC ($EV/runtime_upgrade.log)"; return 1; }
    sleep 12
    fork_height "$SPEC_BEFORE_FORK" > /dev/null || { echo "the spec changed to $spec, but no fork block can be verified"; return 1; }
    echo "spec $SPEC_BEFORE_FORK -> $spec"
}

wait_for_crosser() {
    local exit_code=0
    wait "$CROSSER_PID" || exit_code=$?
    # Failed checks are recorded in group 4; only a crosser that could not run stops the rollout.
    [ "$exit_code" -lt 2 ] || { echo "the crosser's wallet could not run (exit $exit_code, $LOGS/g4-crosser.log)"; return 1; }
    log_group_counts "$LOGS/g4-crosser.log"
}

# --- The rollout --------------------------------------------------------------------------------
operator_step "prepare the new release, $NEW_EXPECTED_NODE_VERSION" prepare_new_release
operator_step "what this release needs before the first wave" operator_step_before_first_wave

bullet "Validators dropping out: one killed during a wave, one offline at the fork block"
wave_number=0
for wave in "${WAVES[@]}"; do
    wave_number=$(( wave_number + 1 ))
    validators_of_wave=()
    for validator in $wave; do
        [ "$validator" = "$LAGGARD" ] || validators_of_wave+=("$validator")
    done
    UPGRADED_VALIDATORS=$(( UPGRADED_VALIDATORS + ${#validators_of_wave[@]} ))
    operator_step "wave $wave_number: validators ${validators_of_wave[*]} move to the new binary" swap_binaries "${validators_of_wave[@]}"
    operator_step "group 3: smoke after wave $wave_number" smoke_after_wave "$wave_number"
    if [ "$wave_number" = 1 ]; then
        check validator-killed-during-wave "a validator still on the old binary is killed mid-rollout: the others carry on" check_validator_killed_during_wave
    else
        check killed-validator-rejoins     "the killed validator restarts on the new binary from its database and catches up" check_killed_validator_rejoins
    fi
done

operator_step "group 4: record the state before the upgrade" record_state_before_upgrade
operator_step "put the runtime wasm in place" place_runtime_wasm
operator_step "group 4: start the crosser's wallet" start_crosser
[ -z "$OFFLINE_AT_FORK" ] || operator_step "stop $(validator_name "$OFFLINE_AT_FORK") before the upgrade" stop_validator_before_upgrade
operator_step "governance submits the runtime upgrade" submit_runtime_upgrade
recall_state

if [ -n "$OFFLINE_AT_FORK" ]; then
    check validator-offline-at-fork        "a validator offline at the fork block: the others enact the upgrade and carry on" check_validator_offline_at_fork
    check offline-validator-catches-up     "the offline validator comes back, imports the fork block itself and catches up"  check_offline_validator_catches_up
fi
if [ -n "$LAGGARD" ]; then
    bullet "A laggard: a validator still on the old binary at the fork"
    check laggard-stops-at-fork            "a validator left on the old binary stops at the fork block; the others finalize"  check_laggard_stops_at_fork
    check laggard-recovers-once-upgraded   "the laggard, upgraded late, catches up through the fork block"                    check_laggard_recovers_once_upgraded
fi

operator_step "group 4: wait for the crosser's wallet to finish" wait_for_crosser

group_finish
