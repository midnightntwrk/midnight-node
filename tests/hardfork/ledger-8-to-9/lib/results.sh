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

# Results tables: one TSV per table, one row per check, and SUMMARY.md over all of them.
#
#   t_table <table> <title>                  start a table (T_APPEND=1 adds to it)
#   t_check <id> <refs> <title> <fn> [args]  run fn as one check; it ends with one of
#                                            t_pass, t_fail, t_warn, t_skip or t_known
#   t_finish                                 print the totals; fails on any FAIL
#
# A FAIL stops the script only under T_FAIL_FAST=1. A KNOWN detail names the filed issue.
# A second verdict in one check is a FAIL; a script that exits before t_finish records
# <table>-ABORTED.

T_TABLE=""; T_TSV=""; T_ID=""; T_REFS=""; T_TITLE=""; T_START=0
T_DONE=""; T_VERDICT=""; T_LAST=""; T_FINISHED=0; T_ABORT_REASON=""; _T_PREV_EXIT=""
T_PASSES=0; T_FAILS=0; T_WARNS=0; T_SKIPS=0; T_KNOWNS=0

# Summary order; other tables follow alphabetically.
RESULT_TABLES=(PRE L8 CLI-L8 HF WAVE-1 WAVE-2 WAVE-3 WAVE-4 WAVE-5 SNAP SNAPDIFF L9 FEAT CLI-L9 HF11 HF12 SAFE SEC)

t_table() {  # <table> <title>
    T_TABLE="$1"; T_TSV="$RESULTS_DIR/$1.tsv"
    T_PASSES=0; T_FAILS=0; T_WARNS=0; T_SKIPS=0; T_KNOWNS=0
    mkdir -p "$RESULTS_DIR"
    if [ "${T_APPEND:-0}" != 1 ] || [ ! -s "$T_TSV" ]; then
        printf 'table\tid\tstatus\trefs\ttitle\tdetail\tseconds\tutc\n' > "$T_TSV"
    fi
    echo "=== $T_TABLE: $2 ($(utc_now)) ==="
    T_FINISHED=0
    local prev; eval "set -- $(trap -p EXIT)"; prev=${3:-}
    [ "$prev" = _t_on_exit ] || { _T_PREV_EXIT=$prev; trap _t_on_exit EXIT; }
}

# A check script that cannot run exits 3; after t_table its table records why.
cannot_run() { echo "$1" >&2; T_ABORT_REASON="could not run: $1"; exit 3; }

_t_on_exit() {
    local rc=$?
    if [ "$T_FINISHED" != 1 ]; then
        [ "$rc" != 0 ] || rc=1
        [ -z "$T_ID" ] || _t_record FAIL "interrupted: the script exited with code $rc"
        T_ID="$T_TABLE-ABORTED"; T_REFS="-"; T_TITLE="the table ran to its end"; T_START=$(date -u '+%s')
        _t_record FAIL "${T_ABORT_REASON:-the script exited with code $rc}; last check started: ${T_LAST:-none}"
        t_finish || true
        echo "=== $T_TABLE aborted (exit $rc)"
    fi
    eval "$_T_PREV_EXIT"
    exit "$rc"
}

t_section() { echo ""; echo "--- $1 ---"; }

_t_record() {  # <status> <detail>
    local st="$1" detail secs
    if [ -z "$T_ID" ] && [ -n "$T_DONE" ]; then T_ID=$T_DONE; st=FAIL; set -- FAIL "second verdict after $T_VERDICT: $1: $2"; fi
    [ -n "$T_ID" ] || { echo "  ($st outside a check: $2)"; return 0; }
    secs=$(( $(date -u '+%s') - T_START ))
    detail=$(printf '%s' "$2" | tr '\t\n' '  ' | cut -c1-500)
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$T_TABLE" "$T_ID" "$st" "$T_REFS" "$T_TITLE" \
        "$detail" "$secs" "$(utc_now)" >> "$T_TSV"
    case "$st" in
        PASS)  T_PASSES=$((T_PASSES + 1)) ;;
        FAIL)  T_FAILS=$((T_FAILS + 1)) ;;
        WARN)  T_WARNS=$((T_WARNS + 1)) ;;
        SKIP)  T_SKIPS=$((T_SKIPS + 1)) ;;
        KNOWN) T_KNOWNS=$((T_KNOWNS + 1)) ;;
    esac
    echo "  $st: ${detail:-ok}"
    T_DONE=$T_ID; T_VERDICT=$st; T_ID=""
}

t_pass()  { _t_record PASS "${1:-}"; }
t_warn()  { _t_record WARN "$1"; }
t_skip()  { _t_record SKIP "$1"; }
# STRICT_KNOWN=1: a release gate that does not accept filed limitations.
t_known() { if [ "${STRICT_KNOWN:-0}" = 1 ]; then _t_record FAIL "KNOWN under STRICT_KNOWN=1: $1"; else _t_record KNOWN "$1"; fi; }
t_info()  { echo "  $1"; }
t_passed() { [ "$T_VERDICT" = PASS ]; }  # the last check passed
t_fail()  {
    _t_record FAIL "$1"
    if [ "${T_FAIL_FAST:-0}" = 1 ]; then
        t_finish || true
        echo "=== $T_TABLE stopped at the failure above (T_FAIL_FAST=1) ==="
        exit 1
    fi
}

t_check() {  # <id> <refs> <title> <function> [args...]
    T_ID="$1"; T_LAST="$1"; T_DONE=""; T_REFS="${2:--}"; T_TITLE="$3"; T_START=$(date -u '+%s'); shift 3
    echo "[$T_ID] $T_TITLE  {$T_REFS}"
    "$@" || true
    [ -z "$T_ID" ] || _t_record FAIL "the check ended without a verdict (last command failed)"
    T_DONE=""
}

table_counts() {  # <log>: "TABLE: n pass, n fail, ..." of every table the log finished
    grep -hoE '^=== [A-Z0-9-]+: [0-9]+ pass, [0-9]+ fail, [0-9]+ warn, [0-9]+ skip, [0-9]+ known' "$1" | sed 's/^=== //' | paste -sd';' - | sed 's/;/; /g'
}

t_finish() {
    T_FINISHED=1
    local total=$(( T_PASSES + T_FAILS + T_WARNS + T_SKIPS + T_KNOWNS ))
    echo ""
    echo "=== $T_TABLE: $T_PASSES pass, $T_FAILS fail, $T_WARNS warn, $T_SKIPS skip, $T_KNOWNS known ($total checks, $T_TSV) ==="
    results_summary >/dev/null 2>&1 || true
    [ "$T_FAILS" -eq 0 ]
}

results_summary() {
    local t f tables=() seen=" "
    for t in "${RESULT_TABLES[@]}"; do
        [ -s "$RESULTS_DIR/$t.tsv" ] && { tables+=("$t"); seen="$seen$t "; }
    done
    for f in "$RESULTS_DIR"/*.tsv; do
        [ -s "$f" ] || continue
        t=$(basename "$f" .tsv)
        case "$seen" in *" $t "*) ;; *) tables+=("$t"); seen="$seen$t " ;; esac
    done
    {
        echo "# Ledger 8 -> 9 hard fork: results"
        echo ""
        [ -s "$RESULTS_DIR/context.md" ] && { cat "$RESULTS_DIR/context.md"; echo ""; }
        echo "Rebuilt $(utc_now) from \`results/*.tsv\`, one row per check."
        echo "Statuses: PASS, FAIL, WARN (ran, unexpected, not a fork defect), SKIP (not run, reason given), KNOWN (filed limitation)."
        echo "Refs: HF-xx = hard-fork test plan item; node#, indexer#, ledger#, midnight-js#, wallet# = public GitHub issue or PR."
        echo ""
        echo "| Table | PASS | FAIL | WARN | SKIP | KNOWN |"
        echo "|---|---|---|---|---|---|"
        for t in "${tables[@]}"; do
            tail -n +2 "$RESULTS_DIR/$t.tsv" | awk -F'\t' -v t="$t" \
                '{ c[$3]++ } END { printf "| %s | %d | %d | %d | %d | %d |\n", t, c["PASS"], c["FAIL"], c["WARN"], c["SKIP"], c["KNOWN"] }'
        done
        for t in "${tables[@]}"; do
            f="$RESULTS_DIR/$t.tsv"
            echo ""
            echo "## $t"
            echo ""
            echo "| ID | Status | Refs | Check | Detail |"
            echo "|---|---|---|---|---|"
            tail -n +2 "$f" | awk -F'\t' '{ gsub(/\|/, "\\|", $6); printf "| %s | %s | %s | %s | %s |\n", $2, $3, $4, $5, $6 }'
        done
    } > "$RESULTS_DIR/SUMMARY.md"
    echo "$RESULTS_DIR/SUMMARY.md"
}
