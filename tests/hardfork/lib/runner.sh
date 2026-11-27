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

# What the local and the network runner share: running one step with its log, the run's
# context for the report, and the report itself.

_RUNNER_LIB="$(dirname "${BASH_SOURCE[0]}")"
source "$_RUNNER_LIB/target.sh"
source "$_RUNNER_LIB/results.sh"
GROUPS_DIR="$SUITE_DIR/groups"

# The groups that run after the fork, in runbook order.
GROUPS_AFTER_FORK="g4 g5 g6 g7 g8 g9 g10"

group_script() {  # <g1..g10> -> the script of that group
    case "$1" in
        g1)  echo "$GROUPS_DIR/g01_release_artefacts.sh" ;;
        g2)  echo "$GROUPS_DIR/g02_old_ledger_features.sh" ;;
        g3)  echo "$GROUPS_DIR/g03_wave_smoke.sh" ;;
        g4)  echo "$GROUPS_DIR/g04_fork_block_integrity.sh" ;;
        g5)  echo "$GROUPS_DIR/g05_state_migration.sh" ;;
        g6)  echo "$GROUPS_DIR/g06_pre_fork_state_after_fork.sh" ;;
        g7)  echo "$GROUPS_DIR/g07_new_ledger_features.sh" ;;
        g8)  echo "$GROUPS_DIR/g08_refusals.sh" ;;
        g9)  echo "$GROUPS_DIR/g09_clients_across_fork.sh" ;;
        g10) echo "$GROUPS_DIR/g10_failure_and_recovery.sh" ;;
        *) return 1 ;;
    esac
}

start_run() {  # <run name>: sets RUN_DIR, where this run's logs go
    RUN_DIR="$RUNS_DIR/$(date -u '+%Y%m%dT%H%M%SZ')${1:+-$1}"
    export RUN_DIR
    mkdir -p "$RUN_DIR"
    ln -sfn "$RUN_DIR" "$RUNS_DIR/latest"
}

# Runs a step, logs it to $RUN_DIR/<name>.log and returns its exit code:
# 0 all checks passed, 1 a check failed, 2 or more the step could not run.
run_step() {  # <name> <command...>
    local name=$1 log exit_code started
    shift
    log="$RUN_DIR/$name.log"; started=$(date +%s)
    echo "--- $name   (log: $log)"
    set +e
    if [ "${QUIET:-0}" = 1 ]; then "$@" >> "$log" 2>&1; exit_code=$?
    else "$@" 2>&1 | tee -a "$log"; exit_code=${PIPESTATUS[0]}; fi
    set -e
    echo "    exit $exit_code after $(( $(date +%s) - started )) s   $(log_group_counts "$log")"
    if [ "$exit_code" != 0 ] && [ "${QUIET:-0}" = 1 ]; then tail -20 "$log"; fi
    return "$exit_code"
}

# What was tested, for the summary and the report.
write_run_context() {  # <target description> <new node ref> <new node image>
    local wallet_sdk midnightjs
    wallet_sdk=$(jq -r '.dependencies["@midnightntwrk/wallet-sdk"]' "$FORK_DIR/clients/package.json")
    midnightjs=$(jq -r '.dependencies["@midnight-ntwrk/midnight-js-contracts"]' "$FORK_DIR/clients/package.json")
    jq -n \
        --arg target "$TARGET" --arg network "$NETWORK_NAME" --arg target_description "$1" \
        --arg old_ledger "$OLD_LEDGER" --arg new_ledger "$NEW_LEDGER" \
        --arg old_ref "$OLD_REF" --arg old_node_image "$OLD_NODE_IMAGE" \
        --arg new_ref "$2" --arg new_node_image "$3" \
        --arg indexer "${INDEXER_SOURCE_COMMIT:-$INDEXER_TAG}" \
        --arg proof_servers "$PROOF_SERVER_OLD_TAG (ledger $OLD_LEDGER), $PROOF_SERVER_NEW_TAG (ledger $NEW_LEDGER)" \
        --arg clients "wallet-sdk $wallet_sdk, midnight-js $midnightjs" \
        --arg suite_commit "$(git -C "$SUITE_DIR" rev-parse --short HEAD)" \
        --arg explorer_block_url "${EXPLORER_BLOCK_URL:-}" \
        '$ARGS.named' > "$RESULTS_DIR/context.json"
    cat > "$RESULTS_DIR/context.md" <<CONTEXT
Target: $1. Ledger $OLD_LEDGER: \`$OLD_REF\` ($OLD_NODE_IMAGE). Ledger $NEW_LEDGER: \`$2\` ($3).
Indexer \`${INDEXER_SOURCE_COMMIT:-$INDEXER_TAG}\`; proof servers \`$PROOF_SERVER_OLD_TAG\` and \`$PROOF_SERVER_NEW_TAG\`; wallet SDK $wallet_sdk, Midnight.js $midnightjs.
CONTEXT
}

write_report() {
    results_summary > /dev/null
    cp "$RESULTS_DIR/SUMMARY.md" "$RUN_DIR/SUMMARY.md"
    echo "=== results: $RESULTS_DIR/SUMMARY.md"
    [ -x "$SUITE_DIR/report/generate.py" ] || return 0
    if python3 "$SUITE_DIR/report/generate.py" "$RESULTS_DIR" --out "$OUT_DIR/report" > /dev/null; then
        echo "=== report: $OUT_DIR/report/REPORT.md and report.html"
    fi
}
