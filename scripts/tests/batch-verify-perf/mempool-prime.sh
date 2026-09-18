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

# Phase 1 of the *mempool* A/B: build a replayable submission workload.
#
#     ./mempool-prime.sh [LOAD_TXS]
#
# The mempool path is measured by submitting transactions to an authoring node, so the workload
# has to be submittable more than once -- otherwise every repeat would have to re-prove, and
# proving is seconds per transaction against milliseconds of mempool validation. Proving inside
# the measured loop would bury the signal completely.
#
# So this script separates the two:
#
#   1. fan out the genesis balance into LOAD_TXS coins, one per derived wallet;
#   2. **archive the chain at that point** -- every repeat restores this exact state, so the
#      transactions below are valid every time;
#   3. build LOAD_TXS proved transactions with `--dest-file` and *do not submit them*.
#
# mempool-benchmark.sh then restores (2) and replays (3) as many times as it likes, with only
# node-side validation inside the timed region.
#
# Output (all under artifacts/):
#   mempool-archive.tar.gz   post-fan-out chain state
#   mempool-archive.meta     height, chainspec, tx count
#   mempool-load.json        the proved transactions, in toolkit `--src-file` form

set -euo pipefail

BV_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "$BV_DIR/lib.sh"

LOAD_TXS="${1:-${LOAD_TXS:-120}}"
TOOLKIT_BIN="${TOOLKIT_BIN:-$REPO_ROOT/target/release/midnight-node-toolkit}"
MEMPOOL_ARCHIVE="${MEMPOOL_ARCHIVE:-$ARTIFACTS_DIR/mempool-archive.tar.gz}"
MEMPOOL_META="${MEMPOOL_META:-$ARTIFACTS_DIR/mempool-archive.meta}"
MEMPOOL_LOAD="${MEMPOOL_LOAD:-$ARTIFACTS_DIR/mempool-load.json}"
PRIME_DIR="${PRIME_DIR:-$LOCAL_WORK_DIR/mempool-prime}"
PRIME_LOG="$PRIME_DIR.log"
PRIME_RPC="http://localhost:${PRODUCER_RPC_HOST_PORT}"
PRIME_WS="ws://127.0.0.1:${PRODUCER_RPC_HOST_PORT}"

require_cmds curl tar
[ -x "$NODE_BIN" ] || die "node binary not found at '$NODE_BIN'"
[ -x "$TOOLKIT_BIN" ] || die "toolkit binary not found at '$TOOLKIT_BIN'"

PRIME_PID=""
cleanup() { [ -n "$PRIME_PID" ] && kill "$PRIME_PID" 2>/dev/null || true; }
trap cleanup EXIT

mkdir -p "$ARTIFACTS_DIR"
rm -rf "$PRIME_DIR"; mkdir -p "$PRIME_DIR"

log "🚀 starting authoring node (logs -> $PRIME_LOG)"
(
  cd "$REPO_ROOT"
  export CFG_PRESET=dev WIPE_CHAIN_STATE=true BASE_PATH="$PRIME_DIR"
  mapfile -t chain_args < <(authoring_chain_args)
  exec "$NODE_BIN" \
    "${chain_args[@]}" \
    --node-key "$DEV_NODE_KEY" \
    --rpc-external --rpc-cors=all --rpc-port "$PRODUCER_RPC_HOST_PORT" \
    --prometheus-external --prometheus-port "$PRODUCER_PROM_PORT" \
    --port "$PRODUCER_P2P_PORT" \
    --state-pruning archive --blocks-pruning archive \
    >"$PRIME_LOG" 2>&1
) &
PRIME_PID=$!

# The toolkit builds against the finalized chain, so wait for GRANDPA rather than the best block.
wait_for_finalized_block "$PRIME_RPC" 1 180 || die "node never finalized a block"

log "🔑 deriving $LOAD_TXS wallets"
mapfile -t SEEDS < <(gen_seeds "$LOAD_TXS" "$SEED_BASE")
ADDRS=()
for s in "${SEEDS[@]}"; do
  ADDRS+=( "$("$TOOLKIT_BIN" show-address --network "$NETWORK_ID" --shielded --seed "$s")" )
done
[ "${#ADDRS[@]}" -eq "$LOAD_TXS" ] || die "expected $LOAD_TXS addresses, got ${#ADDRS[@]}"

# --- step 1: fan out genesis into LOAD_TXS coins ----------------------------
log "🌱 fan-out: $LOAD_TXS coins of $FAN_AMOUNT (chunks of $FANOUT_CHUNK)"
i=0; chunk=0
while [ "$i" -lt "$LOAD_TXS" ]; do
  out_args=(); n=0
  while [ "$i" -lt "$LOAD_TXS" ] && [ "$n" -lt "$FANOUT_CHUNK" ]; do
    out_args+=( --output "addr=${ADDRS[$i]},amount=${FAN_AMOUNT}" )
    i=$(( i + 1 )); n=$(( n + 1 ))
  done
  chunk=$(( chunk + 1 ))
  log "  fan-out chunk $chunk ($n outputs; $i/$LOAD_TXS)"
  "$TOOLKIT_BIN" generate-txs -s "$PRIME_WS" -d "$PRIME_WS" \
      single-tx --source-seed "$GENESIS_SEED" "${out_args[@]}" \
    >>"$PRIME_DIR/toolkit.log" 2>&1 \
    || die "fan-out chunk $chunk failed (see $PRIME_DIR/toolkit.log)"
done

# Let the fan-out finalize before archiving: the replay state must contain every coin the load
# transactions spend, or they would be rejected for reasons that have nothing to do with batching.
HEIGHT="$(best_height "$PRIME_RPC")"; HEIGHT="${HEIGHT:-0}"
log "⏳ settling fan-out (best #$HEIGHT)"
wait_for_finalized_block "$PRIME_RPC" "$HEIGHT" 300 || log "⚠️  fan-out not finalized; archiving anyway"

# --- step 2: build (but do not submit) the load ------------------------------
# `--dest-file` writes the proved transactions instead of sending them, which is what makes the
# workload replayable. Built in chunks for the same reason prime.sh chunks: batch-single-tx builds
# every spec against one snapshot without reserving DUST between them, so concurrent builds need
# distinct DUST outputs from the funder.
log "🧾 building $LOAD_TXS proved txs to $MEMPOOL_LOAD (chunks of $LOAD_CHUNK)"
TRANSFERS="$PRIME_DIR/transfers.json"
CHUNK_FILES=()
j=0; chunk=0
while [ "$j" -lt "$LOAD_TXS" ]; do
  first=1; ccount=0
  {
    printf '['
    while [ "$j" -lt "$LOAD_TXS" ] && [ "$ccount" -lt "$LOAD_CHUNK" ]; do
      [ "$first" -eq 1 ] || printf ','
      first=0
      printf '{"source_seed":"%s","destination_address":"%s","shielded_amount":%s,"funding_seed":"%s"}' \
        "${SEEDS[$j]}" "${ADDRS[$j]}" "$SEND_AMOUNT" "$GENESIS_SEED"
      j=$(( j + 1 )); ccount=$(( ccount + 1 ))
    done
    printf ']\n'
  } > "$TRANSFERS"
  chunk=$(( chunk + 1 ))
  cf="$PRIME_DIR/load-chunk-$chunk.json"
  "$TOOLKIT_BIN" generate-txs -s "$PRIME_WS" --dest-file "$cf" \
      batch-single-tx --transfers-file "$TRANSFERS" \
    >>"$PRIME_DIR/toolkit.log" 2>&1 \
    || die "load chunk $chunk failed (see $PRIME_DIR/toolkit.log)"
  CHUNK_FILES+=( "$cf" )
  log "  built chunk $chunk ($ccount txs; $j/$LOAD_TXS)"
done

# Merge the per-chunk files into one `{"batches":[[...]]}` document. Every transaction goes into a
# single batch so the replay submits them as one burst -- the mempool batcher only has something
# to batch when submissions arrive together.
log "🔗 merging $chunk chunk file(s)"
python3 - "$MEMPOOL_LOAD" "${CHUNK_FILES[@]}" <<'PY'
import json, sys
out_path, chunk_paths = sys.argv[1], sys.argv[2:]
txs = []
for path in chunk_paths:
    doc = json.load(open(path))
    # A one-transaction chunk is written bare; more are wrapped in {"batches": [[...]]}.
    if "batches" in doc:
        for batch in doc["batches"]:
            txs.extend(batch)
    else:
        txs.append(doc)
json.dump({"batches": [txs]}, open(out_path, "w"))
print(f"merged {len(txs)} transactions")
PY
TX_COUNT="$(python3 -c "import json;print(len(json.load(open('$MEMPOOL_LOAD'))['batches'][0]))")"
[ "$TX_COUNT" -eq "$LOAD_TXS" ] || die "expected $LOAD_TXS txs in $MEMPOOL_LOAD, got $TX_COUNT"

# --- archive the pre-load state ---------------------------------------------
log "🛑 stopping node before archiving"
kill "$PRIME_PID" 2>/dev/null || true
wait "$PRIME_PID" 2>/dev/null || true
PRIME_PID=""

HEIGHT="$(sed -n 's/.*Imported #\([0-9]*\).*/\1/p' "$PRIME_LOG" | tail -1)"
HEIGHT="${HEIGHT:-0}"
log "📦 archiving post-fan-out state (height $HEIGHT) -> $MEMPOOL_ARCHIVE"
tar -C "$PRIME_DIR" -czf "$MEMPOOL_ARCHIVE" .
{
  echo "height=$HEIGHT"
  echo "chain=$CHAIN"
  echo "load_txs=$TX_COUNT"
  echo "node_bin=$NODE_BIN"
} > "$MEMPOOL_META"

log "✅ done. archive=$MEMPOOL_ARCHIVE load=$MEMPOOL_LOAD ($TX_COUNT txs, height $HEIGHT)"
log "   next: ./mempool-benchmark.sh"
