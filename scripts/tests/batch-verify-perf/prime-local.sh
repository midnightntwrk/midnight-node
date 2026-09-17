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
# Host-process counterpart of `prime.sh`: builds the same proof-heavy chain and writes the same
# archive, but with locally-built binaries instead of Docker images.
#
# `prime.sh` needs a node and toolkit image built from the branch under test, which in turn needs
# the branch's ledger crates published somewhere the image build can fetch them. When you already
# have `target/release/midnight-node` and `target/release/midnight-node-toolkit`, this skips all
# of that. The archive it produces is byte-compatible with the one `benchmark.sh` restores, so:
#
#     ./prime-local.sh [LOAD_TXS]      # once, slow (every tx is proved locally)
#     ./benchmark.sh                   # local mode A/B, repeatable
#
# Workload (identical to prime.sh, see its "The prime workload" section):
#   1. fan-out  -- split the genesis shielded balance into LOAD_TXS coins across derived wallets
#   2. load     -- LOAD_TXS independent shielded self-transfers, each spending its own coin, all
#                  fee-funded by genesis
#
# The load step is chunked at LOAD_CHUNK (genesis's DUST-output count): one `batch-single-tx`
# invocation can only fee as many transactions as the funder has DUST outputs, and re-fetching
# between chunks picks up the change outputs.

set -euo pipefail

BV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1091
. "$BV_DIR/lib.sh"

LOAD_TXS="${1:-${LOAD_TXS:-60}}"
LOAD_CHUNK="${LOAD_CHUNK:-5}"
FANOUT_CHUNK="${FANOUT_CHUNK:-25}"
FAN_AMOUNT="${FAN_AMOUNT:-100}"
SEND_AMOUNT="${SEND_AMOUNT:-100}"
SEED_BASE="${SEED_BASE:-65536}"
NODE_BIN="${NODE_BIN:-$REPO_ROOT/target/release/midnight-node}"
TOOLKIT_BIN="${TOOLKIT_BIN:-$REPO_ROOT/target/release/midnight-node-toolkit}"

PRIME_DIR="${PRIME_DIR:-$ARTIFACTS_DIR/prime-local}"
PRIME_LOG="$PRIME_DIR/node.log"
PRIME_RPC_PORT="${PRIME_RPC_PORT:-9956}"
PRIME_P2P_PORT="${PRIME_P2P_PORT:-30356}"
PRIME_RPC="http://127.0.0.1:${PRIME_RPC_PORT}"
PRIME_WS="ws://127.0.0.1:${PRIME_RPC_PORT}"
NODE_PID=""

cleanup() {
  if [ -n "$NODE_PID" ] && kill -0 "$NODE_PID" 2>/dev/null; then
    kill "$NODE_PID" 2>/dev/null || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

[ -x "$NODE_BIN" ]    || die "node binary not found at '$NODE_BIN'"
[ -x "$TOOLKIT_BIN" ] || die "toolkit binary not found at '$TOOLKIT_BIN'"

rm -rf "$PRIME_DIR"; mkdir -p "$PRIME_DIR" "$ARTIFACTS_DIR"

mapfile -t CHAIN_ARGS < <(authoring_chain_args)
log "🚀 starting priming node (host process, chain=$CHAIN)"
(
  cd "$REPO_ROOT"
  export CFG_PRESET=dev BASE_PATH="$PRIME_DIR/chain"
  exec "$NODE_BIN" \
    "${CHAIN_ARGS[@]}" --node-key "$DEV_NODE_KEY" \
    --base-path "$PRIME_DIR/chain" \
    --rpc-port "$PRIME_RPC_PORT" --rpc-cors=all \
    --port "$PRIME_P2P_PORT" \
    --state-pruning archive --blocks-pruning archive \
    >"$PRIME_LOG" 2>&1
) &
NODE_PID=$!

# The toolkit builds against the finalized chain, so wait for GRANDPA rather than best block.
log "⏳ waiting for the first finalized block"
wait_for_finalized_block "$PRIME_RPC" 1 180 || { tail -30 "$PRIME_LOG" >&2; die "node never finalized"; }

# --- derive LOAD_TXS distinct wallets ---------------------------------------
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
  "$TOOLKIT_BIN" generate-txs -s "$PRIME_WS" -d "$PRIME_WS" single-tx \
      --source-seed "$GENESIS_SEED" "${out_args[@]}" \
      >>"$PRIME_DIR/toolkit.log" 2>&1 \
    || die "fan-out chunk $chunk failed (see $PRIME_DIR/toolkit.log)"
done

# --- step 2: the proof-tx load ----------------------------------------------
log "🌊 load: $LOAD_TXS proof-txs in chunks of $LOAD_CHUNK"
TRANSFERS="$PRIME_DIR/transfers.json"
total_ok=0; j=0; chunk=0
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
  out="$("$TOOLKIT_BIN" generate-txs -s "$PRIME_WS" -d "$PRIME_WS" -r "${LOAD_RATE:-40}" \
        batch-single-tx --transfers-file "$TRANSFERS" 2>&1)" || true
  printf '%s\n' "$out" >>"$PRIME_DIR/toolkit.log"
  ok="$(printf '%s\n' "$out" | grep -oE '[0-9]+ succeeded' | grep -oE '[0-9]+' | tail -1)"
  ok="${ok:-0}"; total_ok=$(( total_ok + ok ))
  log "  load chunk $chunk: ${ok}/${ccount} ok (${total_ok}/${LOAD_TXS} total)"
done
log "🌊 load complete: ${total_ok}/${LOAD_TXS} proof-txs submitted"

# --- settle finality, stop cleanly, archive ---------------------------------
HEIGHT="$(best_height "$PRIME_RPC")"; HEIGHT="${HEIGHT:-0}"
if [ "$HEIGHT" -gt 2 ]; then
  wait_for_finalized_block "$PRIME_RPC" "$((HEIGHT - 2))" 120 || log "⚠️  finalization lagging; continuing"
fi
HEIGHT="$(best_height "$PRIME_RPC")"; HEIGHT="${HEIGHT:-0}"
[ "$HEIGHT" -ge 3 ] || die "primed chain too short (height=$HEIGHT)"

log "🛑 stopping node to flush the database cleanly"
kill "$NODE_PID" 2>/dev/null || true
wait "$NODE_PID" 2>/dev/null || true
NODE_PID=""

log "📦 archiving base_path -> $ARCHIVE_TAR"
tar czf "$ARCHIVE_TAR" -C "$PRIME_DIR/chain" .
{
  echo "height=$HEIGHT"
  echo "node_image=(local $NODE_BIN)"
  echo "toolkit_image=(local $TOOLKIT_BIN)"
  echo "chain=$CHAIN"
  echo "load_txs=$LOAD_TXS"
  echo "proof_txs=$total_ok"
  echo "shielded=1"
} > "$ARCHIVE_META"

log "✅ primed to height ${HEIGHT} (${total_ok} proof-txs); archive $(du -h "$ARCHIVE_TAR" | cut -f1) at ${ARCHIVE_TAR}"
log "   next: ./benchmark.sh"
