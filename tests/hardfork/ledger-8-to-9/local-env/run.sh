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

# The hard fork on local-env, end to end:
#
#   run.sh                  every phase
#   run.sh l9 features      some phases, on the chain that is already up
#
# MANUAL_UPGRADE=1 stops after the fork phase, which prints what to submit; the run then
# continues with the phases it names.
#
# A phase with failing checks is recorded and the run goes on. A phase that cannot run, or
# a failing env or fork, stops the run and leaves the environment up. HF_QUIET=1 prints
# only the per-phase summaries.
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUITE="$(cd "$HERE/.." && pwd)"

# An idle Mac freezes Docker long enough to kill the local Cardano block producer.
if [ -z "${HF_AWAKE:-}" ] && command -v caffeinate >/dev/null 2>&1; then
    export HF_AWAKE=1; exec caffeinate -dims "$0" "$@"
fi

ALL="env l8 clients-l8 fork snapdiff l9 features clients-l9 resync-validator resync-indexer security safe-mode"
PHASES="${*:-$ALL}"
script_for() {
    case "$1" in
        env) echo "$HERE/start_env.sh" ;;
        l8) echo "$SUITE/checks/l8_baseline.sh" ;;
        clients-l8) echo "$SUITE/checks/client_stack.sh l8" ;;
        fork) echo "$HERE/fork.sh" ;;
        snapdiff) echo "$SUITE/checks/snapshot.sh post" ;;
        l9) echo "$SUITE/checks/l9_preserved.sh" ;;
        features) echo "$SUITE/checks/l9_features.sh" ;;
        clients-l9) echo "$SUITE/checks/client_stack.sh l9" ;;
        resync-validator) echo "$HERE/resync_validator.sh" ;;
        resync-indexer) echo "$HERE/resync_indexer.sh" ;;
        security) echo "$SUITE/checks/security.sh" ;;
        safe-mode) echo "$SUITE/checks/safe_mode.sh" ;;
        *) return 1 ;;
    esac
}
for p in $PHASES; do script_for "$p" > /dev/null || { echo "unknown phase '$p' (phases: $ALL)" >&2; exit 2; }; done

export TARGET=local
source "$SUITE/lib/runner.sh"
RUN_DIR="$RUNS_DIR/$(date -u '+%Y%m%dT%H%M%SZ')"; export RUN_DIR; mkdir -p "$RUN_DIR"; ln -sfn "$RUN_DIR" "$RUNS_DIR/latest"
case " $PHASES " in *" env "*) find "$RESULTS_DIR" -name '*.tsv' -delete 2>/dev/null || true ;; esac
run_context "local-env, five validators with the Cardano stack" "${L9_REF:-PR ${L9_PR:-?}}" "$(l9_node_image 2>/dev/null || echo "built from ${L9_REF:-PR ${L9_PR:-?}}")" "$INDEXER_TAG"

echo "=== local-env hard fork run: $PHASES"
echo "    logs $RUN_DIR, results $RESULTS_DIR/SUMMARY.md"
rc=0 rest="$PHASES"
for p in $PHASES; do
    rest="${rest#"$p"}"; rest="${rest# }"
    read -r -a cmd <<< "$(script_for "$p")"
    code=0; run_step "$p" "${cmd[@]}" || code=$?
    if [ "$code" = 0 ] && [ "$p" = fork ] && [ "${MANUAL_UPGRADE:-0}" = 1 ]; then
        echo "=== stopped after the binary swap (MANUAL_UPGRADE=1): submit the upgrade printed above, then run"
        echo "    $0 ${rest:-snapdiff}"
        break
    fi
    [ "$code" = 0 ] && continue
    rc=1
    if [ "$code" -ge 2 ] || [ "$p" = env ] || [ "$p" = fork ]; then
        echo "=== stopped at phase $p; the environment is left up for inspection"
        break
    fi
done
run_report
exit $rc
