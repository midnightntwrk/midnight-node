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

# A hard fork on local-env, start to end, in the order of the runbook's groups. Here the
# suite also plays the operators: it starts the chain, swaps the binaries in waves, submits
# the runtime upgrade and breaks nodes on purpose.
#
#   run.sh                every step: start g1 g2 rollout g4 g5 g6 g7 g8 g9 g10
#   run.sh g7 g8          some steps, on the chain that is already up
#
#   start     a fresh chain on the old release
#   rollout   the binary waves with group 3 after each, the record of group 4, the runtime
#             upgrade with a wallet transacting across it, and the drop-outs of group 10
#   g10       wipes a validator and the indexer, then the rest of the group
#
# A step with failing checks is recorded and the run goes on. A step that cannot run, a chain
# that did not start or did not fork, stops the run and leaves the environment up.
# QUIET=1 prints only the summary of each step.
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# An idle Mac freezes Docker long enough to kill the local Cardano block producer.
if [ -z "${KEEPING_AWAKE:-}" ] && command -v caffeinate > /dev/null 2>&1; then
    export KEEPING_AWAKE=1
    exec caffeinate -dims "$0" "$@"
fi

export TARGET=local
source "$HERE/../lib/runner.sh"

ALL_STEPS="start g1 g2 rollout $GROUPS_AFTER_FORK"
STEPS="${*:-$ALL_STEPS}"

# One line per command of a step: "<log name> <command...>".
commands_of_step() {  # <step>
    local script
    case "$1" in
        start)   echo "start $HERE/start.sh" ;;
        rollout) echo "rollout $HERE/rollout.sh" ;;
        g4)      echo "g4 $(group_script g4) verify" ;;
        g10)
            echo "g10-wipe-validator $HERE/wipe_validator.sh"
            echo "g10-wipe-indexer $HERE/wipe_indexer.sh"
            echo "g10 $(group_script g10)" ;;
        g3)      return 1 ;;   # group 3 runs inside the rollout, after each wave
        *)       script=$(group_script "$1") || return 1
                 echo "$1 $script" ;;
    esac
}

for step in $STEPS; do
    commands_of_step "$step" > /dev/null || { echo "unknown step '$step' (steps: $ALL_STEPS)" >&2; exit 2; }
done

chain_has_forked() { [ "$(spec_version)" -ge "$NEW_SPEC_FLOOR" ] 2> /dev/null; }

start_run ""
# A fresh chain starts with empty results.
case " $STEPS " in *" start "*) rm -f "$RESULTS_DIR"/*.tsv ;; esac
write_run_context "local-env, five validators with the Cardano stack" "${NEW_REF:-PR ${NEW_PR:-?}}" \
    "$(new_node_image 2> /dev/null || echo "built from ${NEW_REF:-PR ${NEW_PR:-?}}")"

echo "=== hard fork on local-env, ledger $OLD_LEDGER -> $NEW_LEDGER: $STEPS"
echo "    logs: $RUN_DIR"
run_exit_code=0
stopped_at=""
for step in $STEPS; do
    while read -r log_name command; do
        step_exit_code=0
        # shellcheck disable=SC2086
        run_step "$log_name" $command || step_exit_code=$?
        [ "$step_exit_code" = 0 ] && continue
        run_exit_code=1
        # Failed checks are recorded and the run goes on, unless the chain is not there to test.
        if [ "$step_exit_code" -ge 2 ] || [ "$step" = start ] || { [ "$step" = rollout ] && ! chain_has_forked; }; then
            stopped_at="$log_name"
            break
        fi
    done < <(commands_of_step "$step")
    [ -z "$stopped_at" ] || break
done
[ -z "$stopped_at" ] || echo "=== stopped at $stopped_at; the environment is left up for inspection"
[ "$STEPS" = start ] || write_report
exit "$run_exit_code"
