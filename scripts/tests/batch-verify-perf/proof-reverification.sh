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
#
# Counts how many times a Midnight transaction's ZK proofs are verified on a
# single authoring node, from RPC submission to block inclusion.
#
# This is the end-to-end counterpart of the unit test
# `proofs_are_reverified_when_a_transaction_reaches_a_new_block`
# (ledger/src/versions/common/mod.rs): the test pins the mechanism against a
# synthetic state, this measures the real thing on a running node.
#
# Batch verification is left OFF (the shipped default) on purpose — the point is
# to measure the baseline the batch work exists to improve, not the batch path.
#
# ## The signal
#
# The ledger records every *inline* (non-batched) proof verification as
# `ledger_proof_verify_txs_total`, labelled by where it happened:
#
#   mode="inline_mempool"  admitting the transaction to the pool
#                          (`validate_transaction` -> `do_validate_transaction`)
#   mode="inline"          `pre_dispatch` while the block is authored/executed
#                          (`validate_guaranteed_execution`)
#
# Both go through `get_verified_transaction`, which reports a verification only
# when it actually ran the ZK crypto (a strict-cache or proof-cache hit reports
# nothing). So a transaction counted under *both* labels had its proofs verified
# twice on this one node — which is exactly the claim under test.
#
# Everything runs as host processes; no Docker, no images.
#
# Usage:  ./proof-reverification.sh [N_TXS]
#
#   NODE_BIN     node binary     (default target/release/midnight-node)
#   TOOLKIT_BIN  toolkit binary  (default target/release/midnight-node-toolkit)

set -euo pipefail

BV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$BV_DIR/../../.." && pwd)"
# shellcheck disable=SC1091
. "$BV_DIR/../lib/wait-for-node.sh"

N_TXS="${1:-5}"
NODE_BIN="${NODE_BIN:-$REPO_ROOT/target/release/midnight-node}"
TOOLKIT_BIN="${TOOLKIT_BIN:-$REPO_ROOT/target/release/midnight-node-toolkit}"

RPC_PORT="${RPC_PORT:-9955}"
PROM_PORT="${PROM_PORT:-9625}"
P2P_PORT="${P2P_PORT:-30355}"
# The toolkit talks subxt, which requires a websocket URL; the RPC health poll
# and the Prometheus scrape are plain HTTP against the same node.
NODE_WS="ws://127.0.0.1:${RPC_PORT}"
NODE_URL="http://127.0.0.1:${RPC_PORT}"
PROM_URL="http://127.0.0.1:${PROM_PORT}/metrics"

NETWORK_ID="${NETWORK_ID:-undeployed}"
GENESIS_SEED="${GENESIS_SEED:-0000000000000000000000000000000000000000000000000000000000000001}"
DEST_SEED="${DEST_SEED:-0000000000000000000000000000000000000000000000000000000000010001}"
# Shielded (zswap) transfers so each transaction carries contract proofs on top
# of the DUST fee proof.
ADDR_FLAG="${ADDR_FLAG:---shielded}"
SEND_AMOUNT="${SEND_AMOUNT:-100}"
DEV_NODE_KEY="0000000000000000000000000000000000000000000000000000000000000001"

RUN_DIR="$BV_DIR/artifacts/reverify"
NODE_LOG="$RUN_DIR/node.log"
NODE_PID=""

log() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { log "❌ $*"; exit 1; }

cleanup() {
  if [ -n "$NODE_PID" ] && kill -0 "$NODE_PID" 2>/dev/null; then
    log "🧹 stopping node (pid $NODE_PID)"
    kill "$NODE_PID" 2>/dev/null || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

[ -x "$NODE_BIN" ]    || die "node binary not found at '$NODE_BIN' (cargo build --release -p midnight-node)"
[ -x "$TOOLKIT_BIN" ] || die "toolkit binary not found at '$TOOLKIT_BIN' (cargo build --release -p midnight-node-toolkit)"

# Mean of a Prometheus histogram, in milliseconds (0 when absent): `<name>_sum / <name>_count`.
histogram_mean_ms() {
  curl -sf --max-time 3 "$PROM_URL" 2>/dev/null \
    | awk -v n="$1" '
        $1 ~ ("^" n "_sum([{ ]|$)")   { s += $NF }
        $1 ~ ("^" n "_count([{ ]|$)") { c += $NF }
        END { if (c > 0) printf "%.2f", (s / c) * 1000; else printf "0" }'
}

# `ledger_proof_verify_txs_total{mode="<m>"}` as an integer (0 when absent).
proof_verify_count() {
  curl -sf --max-time 3 "$PROM_URL" 2>/dev/null \
    | awk -v m="$1" '
        $0 ~ /^ledger_proof_verify_txs_total\{/ && $0 ~ ("mode=\"" m "\"") { s += $NF }
        END { printf "%d", s+0 }'
}

rm -rf "$RUN_DIR"; mkdir -p "$RUN_DIR"

log "🚀 starting dev node (batch verification OFF — the default)"
(
  cd "$REPO_ROOT"
  export CFG_PRESET=dev BASE_PATH="$RUN_DIR/chain"
  exec "$NODE_BIN" \
    --dev \
    --node-key "$DEV_NODE_KEY" \
    --base-path "$RUN_DIR/chain" \
    --rpc-port "$RPC_PORT" --rpc-cors=all \
    --prometheus-external --prometheus-port "$PROM_PORT" \
    --port "$P2P_PORT" \
    --state-pruning archive --blocks-pruning archive \
    >"$NODE_LOG" 2>&1
) &
NODE_PID=$!

# The toolkit builds against the *finalized* chain, so wait for GRANDPA rather
# than just the best block.
log "⏳ waiting for the first finalized block"
wait_for_finalized_block "$NODE_URL" 1 180 \
  || { tail -30 "$NODE_LOG" >&2; die "node never finalized a block"; }

DEST_ADDR="$("$TOOLKIT_BIN" show-address --network "$NETWORK_ID" "$ADDR_FLAG" --seed "$DEST_SEED")" \
  || die "could not derive destination address"
log "📮 destination: $DEST_ADDR"

before_mempool="$(proof_verify_count inline_mempool)"
before_inline="$(proof_verify_count inline)"
before_batch="$(proof_verify_count batch)"
log "📊 before: inline_mempool=$before_mempool inline=$before_inline batch=$before_batch"

log "💸 submitting $N_TXS shielded transfer(s) (each is proved locally first — slow)"
submitted=0
for i in $(seq 1 "$N_TXS"); do
  if "$TOOLKIT_BIN" generate-txs \
        -s "$NODE_WS" -d "$NODE_WS" \
        single-tx \
          --source-seed "$GENESIS_SEED" \
          --output "addr=${DEST_ADDR},amount=${SEND_AMOUNT}" \
        >>"$RUN_DIR/toolkit.log" 2>&1; then
    submitted=$(( submitted + 1 ))
    log "  tx $i/$N_TXS submitted"
  else
    log "  tx $i/$N_TXS FAILED (see $RUN_DIR/toolkit.log)"
  fi
done
[ "$submitted" -gt 0 ] || { tail -40 "$RUN_DIR/toolkit.log" >&2; die "no transactions were submitted"; }

# Let every submitted transaction reach a block: wait until the inline (block
# execution) counter stops moving for two consecutive polls.
log "⏳ waiting for inclusion"
stable=0; last=-1
for _ in $(seq 1 60); do
  now="$(proof_verify_count inline)"
  if [ "$now" = "$last" ] && [ "$now" -gt "$before_inline" ]; then
    stable=$(( stable + 1 )); [ "$stable" -ge 2 ] && break
  else
    stable=0
  fi
  last="$now"
  sleep 3
done

# Scrape the incremental split before the node is torn down: preparation is the per-transaction
# half (grows with batch size), the fold is the aggregate half (near-constant).
prep_ms="$(histogram_mean_ms midnight_batch_verify_prepare_duration_seconds)"
# The node-side timer spans the whole finalize host call, which also warms the caches and dry-runs
# each transaction's guaranteed segment -- per-transaction work that does NOT amortise. The
# ledger-side `mode="batch"` timer wraps only the aggregate crypto, which is the part that does.
fold_call_ms="$(histogram_mean_ms midnight_batch_verify_duration_seconds)"
fold_crypto_ms="$(curl -sf --max-time 3 "$PROM_URL" 2>/dev/null \
  | awk '''$0 ~ /^ledger_proof_verify_duration_seconds_sum\{/ && $0 ~ /mode="batch"/ { s += $NF }
          $0 ~ /^ledger_proof_verify_duration_seconds_count\{/ && $0 ~ /mode="batch"/ { c += $NF }
          END { if (c > 0) printf "%.2f", (s / c) * 1000; else printf "0" }''')"
batches="$(curl -sf --max-time 3 "$PROM_URL" 2>/dev/null \
  | awk '''$1 ~ /^midnight_batch_verify_batch_size_count/ { s += $NF } END { printf "%d", s+0 }''')"

after_mempool="$(proof_verify_count inline_mempool)"
after_inline="$(proof_verify_count inline)"
after_batch="$(proof_verify_count batch)"
d_mempool=$(( after_mempool - before_mempool ))
d_inline=$(( after_inline - before_inline ))
d_batch=$(( after_batch - before_batch ))
total=$(( d_mempool + d_inline ))

echo
echo "═══════════════ per-transaction proof verifications ═══════════════"
printf 'batch_verify_mempool                : %s\n' "${BATCH_VERIFY_MEMPOOL:-false}"
printf 'batch_verify_block_import           : %s\n' "${BATCH_VERIFY_BLOCK_IMPORT:-false}"
printf 'transactions submitted              : %d\n' "$submitted"
echo   "---"
printf 'mempool admission  (inline_mempool) : %d\n' "$d_mempool"
printf 'block execution    (inline)         : %d\n' "$d_inline"
printf 'batched at ingress (batch)          : %d\n' "$d_batch"
printf 'INLINE proof verifications          : %d\n' "$total"
if [ "$submitted" -gt 0 ]; then
  awk -v t="$total" -v n="$submitted" \
    'BEGIN { printf "inline verifications per tx         : %.2fx\n", t / n }'
fi
printf 'batches dispatched                  : %s\n' "$batches"
printf 'prepare, per tx (on arrival)        : %s ms\n' "$prep_ms"
printf 'fold crypto, per batch              : %s ms   (the part that amortises)\n' "$fold_crypto_ms"
printf 'finalize call, per batch            : %s ms   (incl. cache warming, per-tx)\n' "$fold_call_ms"
echo
echo "node log    : $NODE_LOG"
echo "toolkit log : $RUN_DIR/toolkit.log"
