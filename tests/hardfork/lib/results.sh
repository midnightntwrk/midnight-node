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

# Results: one table per runbook group (results/G2.tsv), one row per check.
#
#   group_start G2 "Old-ledger features before the fork"
#   bullet "Transfers"                                  the runbook bullet the next checks belong to
#   check transfer-shielded "a shielded NIGHT transfer lands" check_transfer_shielded
#   not_covered cardano-side "the bridge and governance sync" "no check reads the Cardano side"
#   group_finish
#
# A check is a function that ends with exactly one verdict: pass, fail, warn, skip or known.
# Its ID is the group plus its name: G2-transfer-shielded.
#
# Fail closed: a check that ends without a verdict is a FAIL, and so is a script that exits
# before group_finish. Running a check again replaces its earlier row.

GROUP=""; GROUP_TSV=""; GROUP_FINISHED=1
BULLET=""; CHECK_NAME_PREFIX=""
CHECK_ID=""; CHECK_TITLE=""; CHECK_STARTED=0
LAST_CHECK_ID=""; LAST_VERDICT=""
CANNOT_RUN_REASON=""
declare -A VERDICT_COUNT=()

RESULTS_HEADER=$'group\tcheck\tstatus\tbullet\ttitle\tdetail\tseconds\tutc'

group_start() {  # <group> <title>
    GROUP="$1"; GROUP_TSV="$RESULTS_DIR/$1.tsv"; GROUP_FINISHED=0
    VERDICT_COUNT=([PASS]=0 [FAIL]=0 [WARN]=0 [SKIP]=0 [KNOWN]=0)
    mkdir -p "$RESULTS_DIR"
    [ -s "$GROUP_TSV" ] || echo "$RESULTS_HEADER" > "$GROUP_TSV"
    echo "=== $GROUP: $2 ($(utc_now)) ==="
    trap _results_on_exit EXIT
}

# A group script that cannot run at all exits 3; its table records why.
cannot_run() {  # <reason>
    echo "$1" >&2
    CANNOT_RUN_REASON="could not run: $1"
    exit 3
}

bullet() {  # <runbook bullet>
    BULLET="$1"
    echo ""
    echo "--- $1 ---"
}

check() {  # <name> <title> <function> [args...]
    CHECK_ID="$GROUP-$CHECK_NAME_PREFIX$1"; CHECK_TITLE="$2"; CHECK_STARTED=$(date -u '+%s')
    LAST_CHECK_ID="$CHECK_ID"; LAST_VERDICT=""
    shift 2
    echo "[$CHECK_ID] $CHECK_TITLE"
    "$@" || true
    [ -n "$LAST_VERDICT" ] || _record FAIL "the check ended without a verdict (its last command failed)"
    CHECK_ID=""
}

# A runbook item nothing tests yet. It keeps a row, so the gap stays visible.
not_covered() {  # <name> <title> <reason>
    check "$1" "$2" skip "not covered yet: $3"
}

pass()  { _record PASS "${1:-}"; }
fail()  { _record FAIL "$1"; }
warn()  { _record WARN "$1"; }
skip()  { _record SKIP "$1"; }
# KNOWN is only for a filed limitation, and only when the evidence matches that issue; the
# detail names it. STRICT_KNOWN=1 is a release gate that accepts none.
known() {
    if [ "${STRICT_KNOWN:-0}" = 1 ]; then _record FAIL "known issue, refused under STRICT_KNOWN=1: $1"
    else _record KNOWN "$1"; fi
}
note() { echo "  $1"; }
last_check_passed() { [ "$LAST_VERDICT" = PASS ]; }

_record() {  # <status> <detail>
    local status="$1" detail="$2" seconds
    if [ -z "$CHECK_ID" ]; then
        echo "  ($status outside a check: $detail)"
        return 0
    fi
    if [ -n "$LAST_VERDICT" ]; then
        _drop_row "$CHECK_ID"
        VERDICT_COUNT[$LAST_VERDICT]=$(( VERDICT_COUNT[$LAST_VERDICT] - 1 ))
        detail="two verdicts in one check: $LAST_VERDICT, then $status: $detail"
        status=FAIL
    fi
    seconds=$(( $(date -u '+%s') - CHECK_STARTED ))
    detail=$(printf '%s' "$detail" | tr '\t\n' '  ' | cut -c1-500)
    _drop_row "$CHECK_ID"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$GROUP" "$CHECK_ID" "$status" "$BULLET" "$CHECK_TITLE" "$detail" "$seconds" "$(utc_now)" >> "$GROUP_TSV"
    VERDICT_COUNT[$status]=$(( VERDICT_COUNT[$status] + 1 ))
    LAST_VERDICT="$status"
    echo "  $status: ${detail:-ok}"
}

_drop_row() {  # <check id>
    awk -F'\t' -v id="$1" '$2 != id' "$GROUP_TSV" > "$GROUP_TSV.tmp" && mv "$GROUP_TSV.tmp" "$GROUP_TSV"
}

group_finish() {
    GROUP_FINISHED=1
    echo ""
    echo "=== $GROUP: ${VERDICT_COUNT[PASS]} pass, ${VERDICT_COUNT[FAIL]} fail, ${VERDICT_COUNT[WARN]} warn," \
        "${VERDICT_COUNT[SKIP]} skip, ${VERDICT_COUNT[KNOWN]} known ($GROUP_TSV) ==="
    results_summary > /dev/null
    [ "${VERDICT_COUNT[FAIL]}" -eq 0 ]
}

_results_on_exit() {
    local exit_code=$?
    [ "$GROUP_FINISHED" = 1 ] && exit "$exit_code"
    [ "$exit_code" != 0 ] || exit_code=1
    [ -z "$CHECK_ID" ] || [ -n "$LAST_VERDICT" ] || _record FAIL "interrupted: the script exited with code $exit_code"
    CHECK_ID="$GROUP-${CHECK_NAME_PREFIX}script-did-not-finish"; CHECK_TITLE="the script ran to its end"
    CHECK_STARTED=$(date -u '+%s'); LAST_VERDICT=""; BULLET=""
    _record FAIL "${CANNOT_RUN_REASON:-the script exited with code $exit_code}; last check started: ${LAST_CHECK_ID:-none}"
    group_finish || true
    echo "=== $GROUP did not finish (exit $exit_code)"
    exit "$exit_code"
}

# --- Summary over every group -------------------------------------------------------
RESULT_TABLES=(SETUP G1 G2 G3 G4 G5 G6 G7 G8 G9 G10)

results_tables() {  # the tables that have rows, in runbook order
    local table
    for table in "${RESULT_TABLES[@]}"; do
        [ -s "$RESULTS_DIR/$table.tsv" ] && echo "$table"
    done
    return 0
}

results_counts() {  # <table> -> "<pass> <fail> <warn> <skip> <known>"
    tail -n +2 "$RESULTS_DIR/$1.tsv" | awk -F'\t' \
        '{ count[$3]++ } END { printf "%d %d %d %d %d\n", count["PASS"], count["FAIL"], count["WARN"], count["SKIP"], count["KNOWN"] }'
}

results_summary() {
    local table
    {
        echo "# Ledger $OLD_LEDGER -> $NEW_LEDGER hard fork: results"
        echo ""
        [ -s "$RESULTS_DIR/context.md" ] && { cat "$RESULTS_DIR/context.md"; echo ""; }
        echo "Rebuilt $(utc_now) from \`results/*.tsv\`, one row per check."
        echo "PASS; FAIL; WARN: ran, unexpected, not a defect of the fork; SKIP: not run, with the reason;"
        echo "KNOWN: a filed limitation, named in the detail."
        echo ""
        echo "| Group | PASS | FAIL | WARN | SKIP | KNOWN |"
        echo "|---|---|---|---|---|---|"
        for table in $(results_tables); do
            read -r passed failed warned skipped known_issues <<< "$(results_counts "$table")"
            echo "| $table | $passed | $failed | $warned | $skipped | $known_issues |"
        done
        for table in $(results_tables); do
            echo ""
            echo "## $table"
            echo ""
            echo "| Check | Status | Runbook bullet | What it checks | Detail |"
            echo "|---|---|---|---|---|"
            tail -n +2 "$RESULTS_DIR/$table.tsv" | awk -F'\t' \
                '{ gsub(/\|/, "\\|", $6); printf "| %s | %s | %s | %s | %s |\n", $2, $3, $4, $5, $6 }'
        done
    } > "$RESULTS_DIR/SUMMARY.md"
    echo "$RESULTS_DIR/SUMMARY.md"
}

# "G2: 40 pass, 0 fail, ..." of every group a log finished.
log_group_counts() {  # <log>
    grep -hoE '^=== [A-Z0-9]+: [0-9]+ pass, [0-9]+ fail, [0-9]+ warn, [0-9]+ skip, [0-9]+ known' "$1" \
        | sed 's/^=== //' | paste -sd';' - | sed 's/;/; /g'
}
