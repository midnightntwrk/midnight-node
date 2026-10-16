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

# What local-env/run.sh and network/run.sh share: the run context, logged steps, the report.
_RUNNER_LIB="$(dirname "${BASH_SOURCE[0]}")"
source "$_RUNNER_LIB/target.sh"
source "$_RUNNER_LIB/results.sh"
SUITE="$(cd "$_RUNNER_LIB/.." && pwd)"

run_context() {  # <description> <ledger-9 ref> <ledger-9 node image> <indexer> [explorer block url]
    local commit pkg="$SUITE/clients/package.json"
    commit=$(git -C "$SUITE" rev-parse --short HEAD)
    jq -n --arg target "$TARGET" --arg network "$NETWORK_NAME" --arg desc "$1" \
        --arg l8_ref "$L8_REF" --arg l8_image "$L8_NODE_IMAGE" \
        --arg ps "$PS_L8_TAG (ledger 8), $PS_L9_TAG (ledger 9)" \
        --arg clients "wallet-sdk $(jq -r '.dependencies["@midnightntwrk/wallet-sdk"]' "$pkg"), midnight-js $(jq -r '.dependencies["@midnight-ntwrk/midnight-js-contracts"]' "$pkg")" \
        --arg commit "$commit" --arg l9_ref "$2" --arg l9_image "$3" --arg indexer "$4" --arg explorer "${5:-}" \
        '{target: $target, network: $network, target_description: $desc, l8_ref: $l8_ref, l8_node_image: $l8_image,
          proof_servers: $ps, clients: $clients, suite_commit: $commit, l9_ref: $l9_ref, l9_node_image: $l9_image,
          indexer: $indexer, explorer_block_url: $explorer}' > "$RESULTS_DIR/context.json"
    cat > "$RESULTS_DIR/context.md" <<CTX
Target: $1. Ledger 8: \`$L8_REF\` ($L8_NODE_IMAGE). Ledger 9: \`$2\` ($3).
Indexer \`$4\`; proof servers \`$PS_L8_TAG\` / \`$PS_L9_TAG\`; suite commit \`$commit\`.
CTX
}

run_step() {  # <name> <command...>: logs to $RUN_DIR/<name>.log, returns the command's exit code
    local name=$1 code started; shift; started=$(date +%s)
    echo "--- $name"
    echo "    log: $RUN_DIR/$name.log"
    set +e
    if [ "${HF_QUIET:-0}" = 1 ]; then "$@" >> "$RUN_DIR/$name.log" 2>&1; code=$?
    else "$@" 2>&1 | tee -a "$RUN_DIR/$name.log"; code=${PIPESTATUS[0]}; fi
    set -e
    echo "    exit $code after $(( $(date +%s) - started )) s  $(table_counts "$RUN_DIR/$name.log")"
    grep -hE '^=== [A-Z0-9-]+ aborted' "$RUN_DIR/$name.log" | sed 's/^/    /' || true
    [ "$code" = 0 ] && return 0
    [ "${HF_QUIET:-0}" = 1 ] && tail -20 "$RUN_DIR/$name.log"
    return "$code"
}

run_report() {
    local args=(--out "$OUT_DIR/report")
    [ -n "${REPORT_NOTES:-}" ] && args+=(--notes "$REPORT_NOTES")
    [ -n "${REPORT_TESTED_BY:-}" ] && args+=(--tested-by "$REPORT_TESTED_BY")
    results_summary > /dev/null; cp "$RESULTS_DIR/SUMMARY.md" "$RUN_DIR/SUMMARY.md"
    python3 "$SUITE/report/generate.py" "$RESULTS_DIR" "${args[@]}" > /dev/null && cp "$OUT_DIR/report/report.html" "$RUN_DIR/"
    echo "=== results: $RESULTS_DIR/SUMMARY.md; report: $OUT_DIR/report/REPORT.md and report.html"
}
