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

# A hard fork on a deployed network, one stage after each step of the operators. The
# operators swap the binaries and submit the runtime upgrade; this only checks.
#
#   run.sh <network> <stage> [argument]      <network>: forks/<fork>/networks/<network>.env, or a file
#
#   When                               Stage            What runs
#   days ahead                         preflight        setup: endpoints, images, caches, clients
#   hours ahead                        fund             setup: test wallets funded and registered for DUST
#   before any node is touched         g1               group 1
#   the day before the first wave      g2               group 2
#   after each binary wave             g3 <wave>        group 3
#   opt-in, any time                   dropout <when>   group 10: a validator pod is killed
#   right before the runtime upgrade   g4-record        group 4: the state is recorded
#   a minute before the upgrade        g4-crosser       group 4: a wallet transacts across the fork
#                                                       (in a second terminal; it returns after the fork)
#   around the upgrade                 watch            one line of fork status; writes no logs
#   after the fork                     after-fork       groups 4 to 10, in order
#   after the fork, one group          g4 ... g10
#
# SEEDS_FILE names the file with the wallet seeds (seeds.env.example). QUIET=1 prints only
# the summary of each step.
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NETWORK="${1:?usage: run.sh <network|settings file> <stage> [argument]}"
STAGE="${2:?the stage is missing (run.sh lists them)}"
ARGUMENT="${3:-}"

source "$HERE/../lib/fork.sh"
if [ -f "$NETWORK" ]; then NETWORK_ENV="$(cd "$(dirname "$NETWORK")" && pwd)/$(basename "$NETWORK")"
else NETWORK_ENV="$FORK_DIR/networks/$NETWORK.env"; fi
[ -s "$NETWORK_ENV" ] || { echo "no network settings at $NETWORK_ENV" >&2; exit 2; }
export TARGET=network NETWORK_ENV

[ "$STAGE" != watch ] || exec "$HERE/watch_fork.sh"
source "$HERE/../lib/runner.sh"

# One line per command of a stage: "<log name> <command...>".
commands_of_stage() {
    local group
    case "$STAGE" in
        preflight)  echo "setup-preflight $HERE/setup.sh preflight" ;;
        fund)       echo "setup-fund $HERE/setup.sh fund" ;;
        g3)         echo "g3-wave${ARGUMENT:?the wave number is missing} $(group_script g3) $ARGUMENT" ;;
        dropout)    echo "g10-dropout-${ARGUMENT:?say when, e.g. wave2} $HERE/dropout.sh $ARGUMENT" ;;
        g4-record)  echo "g4-record $(group_script g4) record" ;;
        g4-crosser) echo "g4-crosser $(group_script g4) crosser" ;;
        g4)         echo "g4 $(group_script g4) verify" ;;
        after-fork)
            for group in $GROUPS_AFTER_FORK; do
                if [ "$group" = g4 ]; then echo "g4 $(group_script g4) verify"; else echo "$group $(group_script "$group")"; fi
            done ;;
        *)          group=$(group_script "$STAGE") || return 1
                    echo "$STAGE $group" ;;
    esac
}
commands_of_stage > /dev/null || { echo "unknown stage '$STAGE' (run.sh lists them)" >&2; exit 2; }

start_run "$STAGE"
write_run_context "$NETWORK_NAME ($NODE_HTTP)" "${NEW_REF:-?}" \
    "${DEPLOYED_NODE_IMAGE:-$(new_node_image 2> /dev/null || echo "built from ${NEW_REF:-?}")}"

echo "=== hard fork on $NETWORK_NAME, ledger $OLD_LEDGER -> $NEW_LEDGER: $STAGE $ARGUMENT"
run_exit_code=0
while read -r log_name command; do
    step_exit_code=0
    # shellcheck disable=SC2086
    run_step "$log_name" $command || step_exit_code=$?
    [ "$step_exit_code" = 0 ] && continue
    run_exit_code=1
    # A step that could not run ends the stage; failed checks are recorded and it goes on.
    if [ "$step_exit_code" -ge 2 ]; then
        echo "=== $log_name could not run"
        run_exit_code=$step_exit_code
        break
    fi
done < <(commands_of_stage)
write_report
exit "$run_exit_code"
