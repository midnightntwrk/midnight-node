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

# The hard fork on a deployed network, one stage per step of the rollout. The operators
# swap binaries and submit the runtime upgrade; this only checks. See README.md.
#
#   run.sh <network|env-file> <stage> [args]
#
#   preflight   before anything else [PRE]
#   baseline    before the first wave [L8, CLI-L8]
#   wave <n>    after binary wave n [WAVE-n]
#   snapshot    right before the runtime upgrade [SNAP]
#   watch       fork status, one line per call (watch_fork.sh); writes no run directory
#   post        after the fork [SNAPDIFF, L9, FEAT, CLI-L9, SEC, SAFE], or one part of it:
#               snapdiff | l9 | features | clients-l9 | security | safe-mode
#
# HF_QUIET=1 prints only the summaries.
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUITE="$(cd "$HERE/.." && pwd)"
NET="${1:?usage: run.sh <network|env-file> <stage> [args]}"; STAGE="${2:?stage missing}"; shift 2
if [ -f "$NET" ]; then NETWORK_ENV="$(cd "$(dirname "$NET")" && pwd)/$(basename "$NET")"; else NETWORK_ENV="$HERE/env/$NET.env"; fi
[ -s "$NETWORK_ENV" ] || { echo "no network env $NETWORK_ENV" >&2; exit 2; }
export TARGET=network NETWORK_ENV
[ "$STAGE" = watch ] && exec "$HERE/watch_fork.sh"

source "$SUITE/lib/runner.sh"
RUN_DIR="$RUNS_DIR/$(date -u '+%Y%m%dT%H%M%SZ')-$STAGE"; export RUN_DIR; mkdir -p "$RUN_DIR"
run_context "$NETWORK_NAME ($NODE_HTTP)" "${L9_REF:-?}" "${DEPLOYED_NODE_IMAGE:-$(l9_node_image 2>/dev/null || echo "built from ${L9_REF:-?}")}" \
    "${INDEXER_SOURCE_COMMIT:-$INDEXER_TAG}" "${EXPLORER_BLOCK_URL:-}"

rc=0
step() {  # a step that cannot run ends the stage
    local code=0; run_step "$@" || code=$?
    [ "$code" -lt 2 ] || { echo "=== $1 could not run"; run_report; exit "$code"; }
    [ "$code" = 0 ] || rc=1
}
case "$STAGE" in
    preflight)  step preflight "$HERE/preflight.sh" ;;
    baseline)   step l8 "$SUITE/checks/l8_baseline.sh"; step clients-l8 "$SUITE/checks/client_stack.sh" l8 ;;
    wave)       step "wave${1:?wave number}" "$SUITE/checks/wave_smoke.sh" "$1" ;;
    snapshot)   step snapshot "$SUITE/checks/snapshot.sh" pre ;;
    post)       step snapdiff "$SUITE/checks/snapshot.sh" post
                step l9 "$SUITE/checks/l9_preserved.sh"
                step features "$SUITE/checks/l9_features.sh"
                step clients-l9 "$SUITE/checks/client_stack.sh" l9
                step security "$SUITE/checks/security.sh"
                step safe-mode "$SUITE/checks/safe_mode.sh" ;;
    snapdiff)   step snapdiff "$SUITE/checks/snapshot.sh" post ;;
    l9)         step l9 "$SUITE/checks/l9_preserved.sh" ;;
    features)   step features "$SUITE/checks/l9_features.sh" ;;
    clients-l9) step clients-l9 "$SUITE/checks/client_stack.sh" l9 ;;
    security)   step security "$SUITE/checks/security.sh" ;;
    safe-mode)  step safe-mode "$SUITE/checks/safe_mode.sh" ;;
    *) echo "unknown stage '$STAGE'" >&2; exit 2 ;;
esac
run_report
exit $rc
