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

# Phase 1 of the batch-verify block-import perf harness: prime a chain with
# proof-bearing transactions and archive it for reuse.
#
#   ./prime.sh NODE_IMAGE TOOLKIT_IMAGE
#
# Both images must be built from THIS branch (e.g. `earthly +node-image` /
# `earthly +toolkit-image`) so they contain the batch-verification code.
#
# Workload (two steps, both submitted to the node so they land in the chain):
#   1. fan-out -- split the genesis shielded balance into LOAD_TXS distinct
#      coins across LOAD_TXS derived wallets (one-or-more multi-output single-tx
#      calls). This is the "prime the chain with lots of outputs" step.
#   2. load    -- `batch-single-tx` builds LOAD_TXS independent shielded
#      transfers, each spending one wallet's own coin (no coin contention) with
#      the single genesis DUST wallet paying every fee (DUST is contention-free).
#
# Result: $ARTIFACTS_DIR/chain-archive.tar.gz (+ .meta) -- a tarball of the
# primed node's base_path. Run once; reuse across many benchmark runs.

set -euo pipefail

BV_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "$BV_DIR/lib.sh"

NODE_IMAGE="${1:-${NODE_IMAGE:-}}"
TOOLKIT_IMAGE="${2:-${TOOLKIT_IMAGE:-}}"
[ -n "$NODE_IMAGE" ] && [ -n "$TOOLKIT_IMAGE" ] \
  || die "usage: prime.sh NODE_IMAGE TOOLKIT_IMAGE"

require_cmds
ensure_network

cleanup() { rm_container "$PRIME_CONTAINER"; }
trap cleanup EXIT

# shielded => zswap proofs (what batch verification accelerates); unshielded is
# offered only as an escape hatch and does NOT exercise the proof path.
if [ "$SHIELDED" = "1" ]; then
  ADDR_FLAG="--shielded"; AMOUNT_KEY="shielded_amount"
else
  ADDR_FLAG="--unshielded"; AMOUNT_KEY="unshielded_amount"
  log "⚠️  SHIELDED=0: unshielded txs carry no ZK proofs -- the benchmark will show no batch-verify signal"
fi

log "🧹 recreating prime volume ($PRIME_VOLUME)"
rm_volume "$PRIME_VOLUME"
docker volume create "$PRIME_VOLUME" >/dev/null

log "🚀 starting dev authority ($NODE_IMAGE)"
rm_container "$PRIME_CONTAINER"
docker run -d --rm \
  --name "$PRIME_CONTAINER" \
  --network "$NETWORK_NAME" \
  -p "${PRODUCER_RPC_HOST_PORT}:9944" \
  -v "$PRIME_VOLUME":"$BASE_PATH_IN" \
  -e CFG_PRESET=dev \
  "$NODE_IMAGE" >/dev/null

PRIME_RPC="http://localhost:${PRODUCER_RPC_HOST_PORT}"
NODE_URL="ws://${PRIME_CONTAINER}:9944"
# The toolkit fetches the *finalized* chain, so wait for GRANDPA to finalize
# past genesis (best-block readiness is not enough -> OnlyGenesisFinalized).
log "⏳ waiting for GRANDPA to finalize past genesis"
wait_for_finalized_block "$PRIME_RPC" 2 180

# --- derive LOAD_TXS distinct source wallets + their addresses --------------
log "🔑 deriving $LOAD_TXS wallet seeds + addresses"
mapfile -t SEEDS < <(gen_seeds "$LOAD_TXS" "$SEED_BASE")
mapfile -t ADDRS < <(derive_addrs "$TOOLKIT_IMAGE" "$ADDR_FLAG" "${SEEDS[@]}")
[ "${#ADDRS[@]}" -eq "$LOAD_TXS" ] \
  || die "expected $LOAD_TXS addresses, got ${#ADDRS[@]}"

# Persistent toolkit cache (ZK proving keys + fetch cache) shared across every
# toolkit invocation, so the tens-of-MB proving keys download once, not per chunk.
docker volume create "$TOOLKIT_CACHE_VOLUME" >/dev/null
RESTORE="$(id -u):$(id -g)"

mkdir -p "$ARTIFACTS_DIR"
TRANSFERS="$ARTIFACTS_DIR/transfers.json"

# --- step 1: fan-out genesis -> LOAD_TXS coins (chunked single-tx) -----------
log "🌱 fan-out: seeding $LOAD_TXS coins of $FAN_AMOUNT from genesis (chunks of $FANOUT_CHUNK)"
i=0
chunk=0
while [ "$i" -lt "$LOAD_TXS" ]; do
  out_args=()
  n=0
  while [ "$i" -lt "$LOAD_TXS" ] && [ "$n" -lt "$FANOUT_CHUNK" ]; do
    out_args+=( --output "addr=${ADDRS[$i]},amount=${FAN_AMOUNT}" )
    i=$(( i + 1 )); n=$(( n + 1 ))
  done
  chunk=$(( chunk + 1 ))
  log "  fan-out chunk $chunk ($n outputs; $i/$LOAD_TXS)"
  docker run --rm -e RUST_BACKTRACE=1 -e RESTORE_OWNER="$RESTORE" \
    -v "$TOOLKIT_CACHE_VOLUME":/.cache \
    --network "$NETWORK_NAME" "$TOOLKIT_IMAGE" \
    generate-txs single-tx \
      --source-seed "$GENESIS_SEED" \
      "${out_args[@]}" \
      -s "$NODE_URL" -d "$NODE_URL" \
    || die "fan-out chunk $chunk failed"
done

# --- step 2: load in DUST-output-sized chunks -------------------------------
# Each batch-single-tx invocation funds all its fees from the single genesis
# wallet, so it is capped at LOAD_CHUNK (genesis's DUST-output count). Re-fetching
# per chunk picks up the change DUST outputs, so the loop scales to any LOAD_TXS.
log "🌊 load: $LOAD_TXS proof-txs in chunks of $LOAD_CHUNK (rate ${LOAD_RATE}/s)"
total_ok=0
j=0
chunk=0
while [ "$j" -lt "$LOAD_TXS" ]; do
  first=1
  ccount=0
  {
    printf '['
    while [ "$j" -lt "$LOAD_TXS" ] && [ "$ccount" -lt "$LOAD_CHUNK" ]; do
      [ "$first" -eq 1 ] || printf ','
      first=0
      # source spends its own fanned-out coin; genesis (funding_seed) pays the fee;
      # destination is the source itself (a self-transfer still produces a proof).
      printf '{"source_seed":"%s","destination_address":"%s","%s":%s,"funding_seed":"%s"}' \
        "${SEEDS[$j]}" "${ADDRS[$j]}" "$AMOUNT_KEY" "$SEND_AMOUNT" "$GENESIS_SEED"
      j=$(( j + 1 )); ccount=$(( ccount + 1 ))
    done
    printf ']\n'
  } > "$TRANSFERS"

  chunk=$(( chunk + 1 ))
  # Capture output to tally succeeded/failed without streaming the toolkit's noise.
  out="$(docker run --rm -e RUST_BACKTRACE=1 -e RESTORE_OWNER="$RESTORE" \
      -v "$ARTIFACTS_DIR":/out -v "$TOOLKIT_CACHE_VOLUME":/.cache \
      --network "$NETWORK_NAME" "$TOOLKIT_IMAGE" \
      generate-txs -s "$NODE_URL" -d "$NODE_URL" -r "$LOAD_RATE" \
      batch-single-tx --transfers-file /out/transfers.json 2>&1)" || true
  ok="$(printf '%s\n' "$out" | grep -oE '[0-9]+ succeeded' | grep -oE '[0-9]+' | tail -1)"
  ok="${ok:-0}"
  total_ok=$(( total_ok + ok ))
  log "  load chunk $chunk: ${ok}/${ccount} ok (${total_ok}/${LOAD_TXS} total)"
done
log "🌊 load complete: ${total_ok}/${LOAD_TXS} proof-txs submitted"

# --- settle finality, then archive ------------------------------------------
HEIGHT="$(best_height "$PRIME_RPC")"; HEIGHT="${HEIGHT:-0}"
log "📈 best height after load: ${HEIGHT}"
if [ "$HEIGHT" -gt 2 ]; then
  wait_for_finalized_block "$PRIME_RPC" "$((HEIGHT - 2))" 120 || log "⚠️  finalization lagging; continuing"
fi
HEIGHT="$(best_height "$PRIME_RPC")"; HEIGHT="${HEIGHT:-0}"
[ "$HEIGHT" -ge 3 ] || die "primed chain too short (height=$HEIGHT); increase LOAD_TXS"

log "🛑 stopping node to flush the database cleanly"
docker stop "$PRIME_CONTAINER" >/dev/null

log "📦 archiving base_path -> $ARCHIVE_TAR"
archive_volume "$PRIME_VOLUME" "$ARCHIVE_TAR"

{
  echo "height=$HEIGHT"
  echo "node_image=$NODE_IMAGE"
  echo "toolkit_image=$TOOLKIT_IMAGE"
  echo "chain=$CHAIN"
  echo "load_txs=$LOAD_TXS"
  echo "proof_txs=$total_ok"
  echo "shielded=$SHIELDED"
} > "$ARCHIVE_META"

ARCHIVE_SIZE="$(du -h "$ARCHIVE_TAR" | cut -f1)"
log "✅ primed to height ${HEIGHT} (${total_ok} proof-txs); archive ${ARCHIVE_SIZE} at ${ARCHIVE_TAR}"
log "   next: ./benchmark.sh ${NODE_IMAGE}"
