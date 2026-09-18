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

# Phase 2 of the mempool A/B: replay a proved workload at an authoring node.
#
#     ./mempool-benchmark.sh
#
# Restores the post-fan-out state built by mempool-prime.sh into a fresh authoring node and
# replays the proved transactions at it, once with BATCH_VERIFY_MEMPOOL=false and once =true.
# Arms are interleaved and counterbalanced, exactly as in benchmark.sh, for the same reason:
# run order otherwise confounds with the treatment.
#
# ## What to read, and what not to
#
# The headline is the **node-side verification budget**, not the wall clock. Submission wall
# clock is bounded by AURA's fixed slot cadence and by how fast the toolkit can push over one
# connection -- neither of which mempool batching changes -- so it is reported as a sanity check
# and nothing more. The counters are what answer the question:
#
#   OFF   ledger_proof_verify_duration_seconds{mode="inline_mempool"}
#   ON    ...{mode="batch"} + ...{mode="batch_prep"}
#
# Those are the mempool half of a transaction's verification cost in each arm. `mode="inline"`
# and `mode="revalidate"` are the authoring half and are reported separately: this node both
# admits and authors, so both halves are present in both arms.

set -euo pipefail

BV_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "$BV_DIR/lib.sh"

TOOLKIT_BIN="${TOOLKIT_BIN:-$REPO_ROOT/target/release/midnight-node-toolkit}"
MEMPOOL_ARCHIVE="${MEMPOOL_ARCHIVE:-$ARTIFACTS_DIR/mempool-archive.tar.gz}"
MEMPOOL_META="${MEMPOOL_META:-$ARTIFACTS_DIR/mempool-archive.meta}"
MEMPOOL_LOAD="${MEMPOOL_LOAD:-$ARTIFACTS_DIR/mempool-load.json}"
NODE_DIR="${MEMPOOL_NODE_DIR:-$LOCAL_WORK_DIR/mempool-node}"
NODE_LOG="$NODE_DIR.log"
NODE_RPC="http://localhost:${SYNCER_RPC_HOST_PORT}"
NODE_WS="ws://127.0.0.1:${SYNCER_RPC_HOST_PORT}"
NODE_PROM="http://localhost:${SYNCER_PROM_HOST_PORT}/metrics"
REPEATS="${REPEATS:-9}"
# Submission rate (txs/sec) handed to the toolkit. High enough that submissions overlap, which is
# the only condition under which the batcher has anything to batch.
REPLAY_RATE="${REPLAY_RATE:-200}"
# Seconds to leave the node running after the last submission before scraping. The pool
# revalidates what it still holds on each block import, so this controls how many revalidations
# the counters capture -- which is the knob that exposes whether the two arms revalidate at the
# same cost (see the README's note on the verification-count asymmetry).
SETTLE_SECS="${SETTLE_SECS:-0}"

require_cmds curl tar
[ -x "$NODE_BIN" ] || die "node binary not found at '$NODE_BIN'"
[ -x "$TOOLKIT_BIN" ] || die "toolkit binary not found at '$TOOLKIT_BIN'"
[ -f "$MEMPOOL_ARCHIVE" ] || die "archive missing: $MEMPOOL_ARCHIVE -- run mempool-prime.sh first"
[ -f "$MEMPOOL_LOAD" ] || die "workload missing: $MEMPOOL_LOAD -- run mempool-prime.sh first"

# The chainspec has to match the one the archive was primed against, for the reason benchmark.sh
# documents: a mismatch is silent, the node simply authors a different chain from its own genesis.
LOAD_TXS_EXPECTED=0
if [ -f "$MEMPOOL_META" ]; then
  META_CHAIN="$(sed -n 's/^chain=//p' "$MEMPOOL_META" | head -1)"
  LOAD_TXS_EXPECTED="$(sed -n 's/^load_txs=//p' "$MEMPOOL_META" | head -1)"
  if [ -n "$META_CHAIN" ] && [ -z "${CHAIN_OVERRIDDEN:-}" ]; then
    [ "$META_CHAIN" = dev ] || [ -f "$META_CHAIN" ] \
      || die "archive was primed with chainspec '$META_CHAIN', which no longer exists"
    CHAIN="$META_CHAIN"
  fi
fi
log "🔗 chainspec: $CHAIN"
log "🧾 workload: $MEMPOOL_LOAD (${LOAD_TXS_EXPECTED:-?} txs), replay rate ${REPLAY_RATE}/s"

NODE_PID=""
cleanup() { [ -n "$NODE_PID" ] && kill "$NODE_PID" 2>/dev/null || true; }
trap cleanup EXIT

# Starts a fresh authoring node on the restored state. $1 = BATCH_VERIFY_MEMPOOL value.
start_node() {
  local flag="$1"
  host_restore_dir "$NODE_DIR" "$MEMPOOL_ARCHIVE"
  (
    cd "$REPO_ROOT"
    export CFG_PRESET=dev BASE_PATH="$NODE_DIR"
    local v
    for v in BATCH_VERIFY_BLOCK_IMPORT BATCH_VERIFY_MAX_BATCH_SIZE \
             BATCH_VERIFY_TARGET_BATCH_SIZE BATCH_VERIFY_MAX_AGE_MS \
             BATCH_VERIFY_WORKERS BATCH_VERIFY_QUEUE_CAPACITY; do
      [ -n "${!v:-}" ] && export "$v=${!v}"
    done
    # Exported last so it wins over any passthrough of the same name.
    export BATCH_VERIFY_MEMPOOL="$flag"
    mapfile -t chain_args < <(authoring_chain_args)
    exec "$NODE_BIN" \
      "${chain_args[@]}" \
      --node-key "$DEV_NODE_KEY" \
      --rpc-external --rpc-cors=all --rpc-port "$SYNCER_RPC_HOST_PORT" \
      --prometheus-external --prometheus-port "$SYNCER_PROM_HOST_PORT" \
      --port "$SYNCER_P2P_PORT" \
      --state-pruning archive --blocks-pruning archive \
      >"$NODE_LOG" 2>&1
  ) &
  NODE_PID=$!
}

stop_node() {
  [ -n "$NODE_PID" ] || return 0
  kill "$NODE_PID" 2>/dev/null || true
  wait "$NODE_PID" 2>/dev/null || true
  NODE_PID=""
}

# One measured replay. $1 = flag. Sets RESULT_SECS and RESULT_OK.
run_replay() {
  local flag="$1"
  start_node "$flag"
  # The toolkit builds against the finalized chain, so wait for GRANDPA before submitting.
  wait_for_finalized_block "$NODE_RPC" 1 180 \
    || { stop_node; die "node never finalized a block (see $NODE_LOG)"; }

  local started ended out
  started="$(now_ms)"
  out="$("$TOOLKIT_BIN" generate-txs --src-file "$MEMPOOL_LOAD" -d "$NODE_WS" \
        -r "$REPLAY_RATE" --ignore-block-context send 2>&1)" || true
  ended="$(now_ms)"
  printf '%s\n' "$out" >"$NODE_DIR.replay.log"

  RESULT_SECS="$(awk -v a="$started" -v b="$ended" 'BEGIN{printf "%.2f", (b-a)/1000}')"
  # `send` prints one SENT per accepted submission; a rejected one never reaches that line.
  RESULT_OK="$(printf '%s\n' "$out" | grep -c '^SENT' || true)"

  if [ "$SETTLE_SECS" -gt 0 ]; then
    log "   settling ${SETTLE_SECS}s so pool revalidation is counted"
    sleep "$SETTLE_SECS"
  fi
  # `scrape_batch_metrics` prints to stdout, so the file has to come from a redirect.
  scrape_batch_metrics "$NODE_PROM" > "$ARTIFACTS_DIR/mempool-metrics-$flag.txt" || true
  if [ ! -s "$ARTIFACTS_DIR/mempool-metrics-$flag.txt" ]; then
    log "⚠️  no metrics scraped for flag=$flag — the result section below will be empty"
  fi
  stop_node
}

OFF_TIMES=(); ON_TIMES=(); OFF_OK=(); ON_OK=()

run_one() {
  local flag="$1" r="$2"
  case "$flag" in
    false) log "════════ OFF (BATCH_VERIFY_MEMPOOL=false) run $r/$REPEATS ════════" ;;
    true)  log "════════ ON  (BATCH_VERIFY_MEMPOOL=true)  run $r/$REPEATS ════════" ;;
  esac
  run_replay "$flag"
  case "$flag" in
    false) OFF_TIMES+=( "$RESULT_SECS" ); OFF_OK+=( "$RESULT_OK" ) ;;
    true)  ON_TIMES+=( "$RESULT_SECS" );  ON_OK+=( "$RESULT_OK" ) ;;
  esac
  log "   ${flag}: ${RESULT_SECS}s, ${RESULT_OK} submitted"
}

# Interleaved and counterbalanced, as in benchmark.sh.
for r in $(seq 1 "$REPEATS"); do
  if [ $(( r % 2 )) -eq 1 ]; then
    run_one false "$r"; run_one true "$r"
  else
    run_one true "$r"; run_one false "$r"
  fi
done

# --- report ------------------------------------------------------------------
OFF_METRICS="$ARTIFACTS_DIR/mempool-metrics-false.txt"
ON_METRICS="$ARTIFACTS_DIR/mempool-metrics-true.txt"
PV_DUR=ledger_proof_verify_duration_seconds_sum
PV_TXS=ledger_proof_verify_txs_total

m() { metric_mode "$1" "$2" "$3"; }

echo
echo "═══════════════ batch-verify mempool benchmark ═══════════════"
printf 'node binary            : %s\n' "$NODE_BIN"
printf 'workload               : %s txs, replay rate %s/s   (repeats: %s)\n' \
  "${LOAD_TXS_EXPECTED:-?}" "$REPLAY_RATE" "$REPEATS"
printf 'A/B variable           : BATCH_VERIFY_MEMPOOL\n'
printf 'submitted (off / on)   : [%s] / [%s]\n' "${OFF_OK[*]}" "${ON_OK[*]}"
echo
echo "--- submission wall clock (cadence-bound; a sanity check, not the result) ---"
printf 'OFF (=false)           : %s   [%s]\n' "$(stats "${OFF_TIMES[@]}")" "${OFF_TIMES[*]}"
printf 'ON  (=true)            : %s   [%s]\n' "$(stats "${ON_TIMES[@]}")" "${ON_TIMES[*]}"
report_paired s "${#OFF_TIMES[@]}" "${OFF_TIMES[@]}" "${ON_TIMES[@]}"

echo
echo "--- mempool verification cost (the result) ---"
# The ON arm's expensive per-proof work lives in `prepare()`, which the *node* times
# (`midnight_batch_verify_prepare_duration_seconds`) and which has no ledger-side `mode=` counter
# of its own. Totalling only the ledger-side `batch` + `batch_prep` modes therefore omits the bulk
# of the cost and overstates the ON arm by roughly 4x. Both node-side timers are used here, and
# the ledger-side modes are printed underneath as a subset, clearly labelled.
awk -v ois="$(m "$OFF_METRICS" "$PV_DUR" inline_mempool)" \
    -v oit="$(m "$OFF_METRICS" "$PV_TXS" inline_mempool)" \
    -v nprep="$(metric_sumf "$ON_METRICS" prepare_duration_seconds_sum)" \
    -v nfin="$(metric_sumf "$ON_METRICS" duration_seconds_sum)" \
    -v ntx="$(metric_sumf "$ON_METRICS" txs_total)" \
    -v lb="$(m "$ON_METRICS" "$PV_DUR" batch)" \
    -v lp="$(m "$ON_METRICS" "$PV_DUR" batch_prep)" 'BEGIN {
  if (oit <= 0 && ntx <= 0) {
    print "  (no mempool verification samples in either arm — the submissions never reached"
    print "   the ingress path; check the replay log before reading anything else)"
    exit
  }
  off_tx = (oit > 0) ? ois / oit : 0
  on_tot = nprep + nfin
  on_tx  = (ntx > 0) ? on_tot / ntx : 0
  printf "  OFF inline_mempool     : %8.3f ms/tx   (%d txs, %.3fs)\n", off_tx*1000, oit, ois
  printf "  ON  prepare + finalize : %8.3f ms/tx   (%d txs, %.3fs = %.3fs prepare + %.3fs finalize)\n", \
         on_tx*1000, ntx, on_tot, nprep, nfin
  if (off_tx > 0 && on_tx > 0)
    printf "  ratio                  : %.2fx  (>1 means batching admits more cheaply)\n", off_tx/on_tx
  printf "    ledger-side subset of the ON figure: %.3fs aggregate fold + %.3fs non-crypto well_formed\n", lb, lp
  printf "    (that subset alone would read %.2fx — it excludes the per-proof preparation)\n", \
         (lb + lp > 0) ? off_tx / ((lb + lp) / ntx) : 0
  if (oit > 0 && ntx > 0 && (oit > ntx * 1.1 || ntx > oit * 1.1))
    printf "  ⚠️  the arms verified different transaction counts (%d vs %d) — not comparable\n", oit, ntx
}'

echo
echo "--- authoring-side cost, both arms (this node admits AND authors) ---"
for arm in false:$OFF_METRICS true:$ON_METRICS; do
  f="${arm#*:}"; a="${arm%%:*}"
  printf '  %-5s inline=%ss/%s tx   revalidate=%ss/%s tx\n' "$a" \
    "$(m "$f" "$PV_DUR" inline)" "$(m "$f" "$PV_TXS" inline)" \
    "$(m "$f" "$PV_DUR" revalidate)" "$(m "$f" "$PV_TXS" revalidate)"
done

echo
echo "--- batcher behaviour (ON arm) ---"
grep -E "midnight_batch_verify_(batch_size|batches_total|dispatch|queue|fallback|txs_total)" \
  "$ON_METRICS" 2>/dev/null || echo "(none scraped)"
echo
echo "--- raw ON mempool counters ---"; grep -E "ledger_proof_verify" "$ON_METRICS" 2>/dev/null || true
echo "--- raw OFF mempool counters ---"; grep -E "ledger_proof_verify" "$OFF_METRICS" 2>/dev/null || true
echo "═══════════════════════════════════════════════════════════════"
