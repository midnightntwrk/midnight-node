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

# Phase 2 of the batch-verify block-import perf harness: A/B benchmark.
#
#   ./benchmark.sh NODE_IMAGE     # docker mode: run the given node image
#   ./benchmark.sh                # local mode: run a locally-built node binary
#                                 #   (NODE_BIN, default target/release/midnight-node)
#
# Restores the archive built by prime.sh into a non-authoring *producer*, then
# full-syncs a fresh *node-under-test* from it twice -- once with
# BATCH_VERIFY_BLOCK_IMPORT=false, once =true -- and reports the sync time for
# each. The delta is the batch-verification speedup on the block-import path.
#
# Two run modes, chosen exactly like scripts/tests/toolkit-tokens-minter-e2e.sh:
# passing a node image runs containers; passing nothing runs the local binary as
# host processes (the image base's glibc is too old for a freshly-built host
# binary, so local mode never containerises it). Both modes share the polling,
# metrics-scrape and reporting logic below; only how a node is launched differs.

set -euo pipefail

BV_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1091
. "$BV_DIR/lib.sh"

NODE_IMAGE="${1:-${NODE_IMAGE:-}}"
if [ -n "$NODE_IMAGE" ]; then
  MODE=docker
  log "🧱 mode: docker  (node image: $NODE_IMAGE)"
else
  MODE=local
  [ -x "$NODE_BIN" ] || die "local mode: node binary not found/executable at '$NODE_BIN' \
-- build it (\`cargo build --release\`) or set NODE_BIN=<path>"
  log "🧱 mode: local   (node binary: $NODE_BIN)"
  log "ℹ️  local mode reuses the existing archive; its genesis must match this binary's \
dev chainspec (re-prime with the same binary if the syncer can't find the producer's chain)."
fi

if [ "$MODE" = docker ]; then
  require_cmds docker curl
  ensure_network
else
  require_cmds curl tar
fi
[ -f "$ARCHIVE_TAR" ] || die "archive missing: $ARCHIVE_TAR -- run prime.sh first"

# Target height: recorded by prime.sh, overridable via TARGET_HEIGHT env.
if [ -f "$ARCHIVE_META" ]; then
  META_HEIGHT="$(sed -n 's/^height=//p' "$ARCHIVE_META" | head -1)"
  [ -n "$META_HEIGHT" ] && TARGET_HEIGHT="$META_HEIGHT"
fi
log "🎯 target sync height: $TARGET_HEIGHT"

PRODUCER_PID=""
SYNCER_PID=""
cleanup() {
  if [ "$MODE" = docker ]; then
    rm_container "$SYNCER_CONTAINER"
    rm_container "$PRODUCER_CONTAINER"
    rm_volume "$SYNCER_VOLUME"
  else
    [ -n "$SYNCER_PID" ]   && kill "$SYNCER_PID"   2>/dev/null || true
    [ -n "$PRODUCER_PID" ] && kill "$PRODUCER_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# --- mode-aware node runners ------------------------------------------------
# Each pair (docker vs local) launches the same node with the same effective
# config; only the transport differs (container on a docker network vs. host
# process on localhost). In local mode the dev preset's `args` array is replaced
# by the CLI (config layering: preset < cli), so the producer must re-supply the
# preset's authoring flags itself; BASE_PATH is an env override read separately
# from that array, so it still points the node at the restored dir.

start_producer() {
  if [ "$MODE" = docker ]; then
    log "📦 restoring producer base_path from archive (docker volume)"
    restore_volume "$PRODUCER_VOLUME" "$ARCHIVE_TAR"
    log "🚀 starting producer container (seeds the network with the primed chain)"
    rm_container "$PRODUCER_CONTAINER"
    docker run -d --rm \
      --name "$PRODUCER_CONTAINER" \
      --network "$NETWORK_NAME" \
      -p "${PRODUCER_RPC_HOST_PORT}:9944" \
      -v "$PRODUCER_VOLUME":"$BASE_PATH_IN" \
      -e CFG_PRESET=dev \
      "$NODE_IMAGE" >/dev/null
  else
    log "📦 restoring producer base_path from archive (host dir $PRODUCER_DIR)"
    host_restore_dir "$PRODUCER_DIR" "$ARCHIVE_TAR"
    log "🚀 starting producer (local binary, logs -> $PRODUCER_LOG)"
    # Replicates the container's `CFG_PRESET=dev` + dev-preset `args` (see
    # res/cfg/dev.toml) plus explicit ports so it never collides with the syncer.
    # CWD = repo root so the preset's relative res/ paths resolve; BASE_PATH env
    # points it at the restored dir (proven equivalent to the container).
    (
      cd "$REPO_ROOT"
      export CFG_PRESET=dev BASE_PATH="$PRODUCER_DIR"
      exec "$NODE_BIN" \
        --dev \
        --node-key "$DEV_NODE_KEY" \
        --rpc-external --rpc-cors=all --rpc-port "$PRODUCER_RPC_HOST_PORT" \
        --prometheus-external --prometheus-port "$PRODUCER_PROM_PORT" \
        --port "$PRODUCER_P2P_PORT" \
        --state-pruning archive --blocks-pruning archive \
        >"$PRODUCER_LOG" 2>&1
    ) &
    PRODUCER_PID=$!
  fi
}

# Print the producer's startup log (for peer-id extraction / diagnostics).
producer_log() {
  if [ "$MODE" = docker ]; then docker logs "$PRODUCER_CONTAINER" 2>&1
  else cat "$PRODUCER_LOG" 2>/dev/null; fi
}

# Launch a fresh syncer (node-under-test). $1 = flag (false|true), $2 = node key.
start_syncer() {
  local flag="$1" node_key="$2"
  if [ "$MODE" = docker ]; then
    rm_container "$SYNCER_CONTAINER"
    rm_volume "$SYNCER_VOLUME"
    docker volume create "$SYNCER_VOLUME" >/dev/null
    local tuning=() v
    for v in BATCH_VERIFY_MAX_BATCH_SIZE BATCH_VERIFY_TARGET_BATCH_SIZE \
             BATCH_VERIFY_MAX_AGE_MS BATCH_VERIFY_WORKERS BATCH_VERIFY_QUEUE_CAPACITY; do
      [ -n "${!v:-}" ] && tuning+=( -e "$v=${!v}" )
    done
    # CFG_PRESET=dev keeps the mock main-chain-follower config; the explicit run
    # args replace the preset's `--dev ...` (so no dev keys / no authoring) and
    # make this a plain full-sync node whose import queue runs the batch verifier.
    docker run -d --rm \
      --name "$SYNCER_CONTAINER" \
      --network "$NETWORK_NAME" \
      -p "${SYNCER_RPC_HOST_PORT}:9944" \
      -p "${SYNCER_PROM_HOST_PORT}:9615" \
      -v "$SYNCER_VOLUME":"$BASE_PATH_IN" \
      -e CFG_PRESET=dev \
      -e WIPE_CHAIN_STATE=true \
      -e "BATCH_VERIFY_BLOCK_IMPORT=$flag" \
      "${tuning[@]}" \
      "$NODE_IMAGE" \
        --chain "$CHAIN" \
        --node-key "$node_key" \
        --base-path "$BASE_PATH_IN" \
        --bootnodes "$BOOTNODE" \
        --sync full \
        --no-mdns \
        --rpc-external --rpc-cors=all --rpc-port 9944 \
        --prometheus-external --prometheus-port 9615 \
        --state-pruning archive --blocks-pruning archive >/dev/null
  else
    rm -rf "$SYNCER_DIR"
    mkdir -p "$SYNCER_DIR"
    (
      cd "$REPO_ROOT"
      export CFG_PRESET=dev WIPE_CHAIN_STATE=true BATCH_VERIFY_BLOCK_IMPORT="$flag"
      export BASE_PATH="$SYNCER_DIR"
      local v
      for v in BATCH_VERIFY_MAX_BATCH_SIZE BATCH_VERIFY_TARGET_BATCH_SIZE \
               BATCH_VERIFY_MAX_AGE_MS BATCH_VERIFY_WORKERS BATCH_VERIFY_QUEUE_CAPACITY; do
        [ -n "${!v:-}" ] && export "$v=${!v}"
      done
      exec "$NODE_BIN" \
        --chain "$CHAIN" \
        --node-key "$node_key" \
        --base-path "$SYNCER_DIR" \
        --bootnodes "$BOOTNODE" \
        --sync full \
        --no-mdns \
        --rpc-external --rpc-cors=all --rpc-port "$SYNCER_RPC_HOST_PORT" \
        --prometheus-external --prometheus-port "$SYNCER_PROM_HOST_PORT" \
        --port "$SYNCER_P2P_PORT" \
        --state-pruning archive --blocks-pruning archive \
        >"$SYNCER_LOG" 2>&1
    ) &
    SYNCER_PID=$!
  fi
}

# True while the current syncer is still running.
syncer_alive() {
  if [ "$MODE" = docker ]; then
    docker ps --format '{{.Names}}' | grep -q "^${SYNCER_CONTAINER}$"
  else
    [ -n "$SYNCER_PID" ] && kill -0 "$SYNCER_PID" 2>/dev/null
  fi
}

# Last lines of the current syncer's log, for failure diagnostics.
syncer_logs_tail() {
  if [ "$MODE" = docker ]; then docker logs --tail 80 "$SYNCER_CONTAINER" 2>&1 || true
  else tail -n 80 "$SYNCER_LOG" 2>/dev/null || true; fi
}

# Tear the current syncer down and (local mode) wait for it to release its ports
# before the next leg reuses them.
stop_syncer() {
  if [ "$MODE" = docker ]; then
    rm_container "$SYNCER_CONTAINER"
  else
    [ -n "$SYNCER_PID" ] && kill "$SYNCER_PID" 2>/dev/null || true
    [ -n "$SYNCER_PID" ] && wait "$SYNCER_PID" 2>/dev/null || true
    SYNCER_PID=""
  fi
}

# --- producer: restore + start once (serves the immutable blocks 1..N) ------
# The producer keeps the dev preset, so it authors past N -- harmless, since the
# syncer targets a fixed height and blocks 1..N are immutable history it serves.
start_producer

log "⏳ waiting for producer to load the archive (best >= $TARGET_HEIGHT)"
if ! wait_for_unfinalized_block "http://localhost:${PRODUCER_RPC_HOST_PORT}" "$TARGET_HEIGHT" 180; then
  producer_log | tail -80 >&2 || true
  die "producer did not reach height $TARGET_HEIGHT"
fi

# Build the bootnode multiaddr from the producer's actual peer id (logged at
# startup), so we never depend on a hard-coded node-key -> peer-id mapping.
PEER_ID="$(producer_log | grep -oE 'Local node identity is: 12D3[A-Za-z0-9]+' | head -1 | awk '{print $NF}')"
[ -n "$PEER_ID" ] || die "could not read producer peer id from logs"
if [ "$MODE" = docker ]; then
  BOOTNODE="/dns4/${PRODUCER_CONTAINER}/tcp/${P2P_PORT}/p2p/${PEER_ID}"
else
  BOOTNODE="/ip4/127.0.0.1/tcp/${PRODUCER_P2P_PORT}/p2p/${PEER_ID}"
fi
log "🔗 bootnode: $BOOTNODE"

# --- one A/B leg: full-sync a fresh node-under-test, time it ---------------
RESULT_SECS=0
run_sync() { # $1 = flag (false|true)
  local flag="$1"

  # A fresh random node key per run: reusing one peer id across rapid
  # teardown/reconnect cycles makes the producer reject the returning peer
  # (stuck at "0 peers"), so each syncer must present a distinct identity.
  local node_key
  node_key=$(head -c 32 /dev/urandom | hexdump -v -e '/1 "%02x"')

  local t0 t0_ms t1_ms
  t0=$(date +%s)        # coarse, for the watchdog/stall checks
  t0_ms=$(date +%s%3N)  # precise, for the reported sync time
  start_syncer "$flag" "$node_key"

  local rpc="http://localhost:${SYNCER_RPC_HOST_PORT}"
  local last=0 last_progress now h
  last_progress=$(date +%s)
  while :; do
    now=$(date +%s)
    (( now - t0 > SYNC_TIMEOUT_SECS )) \
      && { syncer_logs_tail >&2; die "sync timeout (flag=$flag) at height $last"; }
    syncer_alive \
      || { syncer_logs_tail >&2; die "syncer exited early (flag=$flag) at height $last"; }
    h="$(best_height "$rpc")"; h="${h:-0}"
    if (( h > last )); then last=$h; last_progress=$now; log "  [flag=$flag] best #$last"; fi
    if (( last >= TARGET_HEIGHT )); then t1_ms=$(date +%s%3N); break; fi
    (( now - last_progress > STALL_TIMEOUT_SECS )) \
      && { syncer_logs_tail >&2; die "sync stalled at #$last (flag=$flag)"; }
    sleep "$POLL_INTERVAL_SECS"
  done

  # capture metrics while the node is still up
  scrape_batch_metrics "http://localhost:${SYNCER_PROM_HOST_PORT}/metrics" \
    > "$ARTIFACTS_DIR/metrics-${flag}.txt" || true
  stop_syncer
  RESULT_SECS=$(awk -v a="$t0_ms" -v b="$t1_ms" 'BEGIN { printf "%.1f", (b - a) / 1000 }')
}

# Each config is synced REPEATS times; a fresh full sync is only seconds, so the
# minimum across runs is the cleanest signal (least startup/connection noise).
REPEATS="${REPEATS:-3}"
OFF_TIMES=()
ON_TIMES=()
for r in $(seq 1 "$REPEATS"); do
  log "════════ OFF (inline) run $r/$REPEATS ════════"
  run_sync false
  OFF_TIMES+=( "$RESULT_SECS" )
  log "   OFF #$r: ${RESULT_SECS}s"
done
for r in $(seq 1 "$REPEATS"); do
  log "════════ ON (batch) run $r/$REPEATS ════════"
  run_sync true
  ON_TIMES+=( "$RESULT_SECS" )
  log "   ON #$r: ${RESULT_SECS}s"
done

# min + mean of a list of floats
stats() { awk 'NR==1{m=$1} {s+=$1; if($1<m)m=$1} END{printf "min=%.1fs mean=%.1fs", m, s/NR}' <<<"$(printf '%s\n' "$@")"; }
OFF_MIN="$(printf '%s\n' "${OFF_TIMES[@]}" | awk 'NR==1{m=$1} $1<m{m=$1} END{printf "%.1f", m}')"
ON_MIN="$(printf '%s\n' "${ON_TIMES[@]}" | awk 'NR==1{m=$1} $1<m{m=$1} END{printf "%.1f", m}')"

# --- report (stdout) -------------------------------------------------------
echo
echo "═══════════════ batch-verify block-import benchmark ═══════════════"
if [ "$MODE" = docker ]; then
  printf 'node image             : %s\n' "$NODE_IMAGE"
else
  printf 'node binary (local)    : %s\n' "$NODE_BIN"
fi
printf 'blocks synced          : %s   (repeats: %s)\n' "$TARGET_HEIGHT" "$REPEATS"
printf 'OFF (inline verify)    : %s   [%s]\n' "$(stats "${OFF_TIMES[@]}")" "${OFF_TIMES[*]}"
printf 'ON  (batch verify)     : %s   [%s]\n' "$(stats "${ON_TIMES[@]}")" "${ON_TIMES[*]}"
awk -v off="$OFF_MIN" -v on="$ON_MIN" 'BEGIN {
  printf "delta (min off-on)     : %.1fs\n", off - on
  if (on > 0) printf "speedup (min off/on)   : %.2fx\n", off / on
}'
echo
# Coverage check. The block-import path records batches_total/txs_total/batch_size
# (via BatchVerifier::observe_batch) but NOT duration_seconds or fallback_total —
# those are mempool-only (node/src/batch_chain_api.rs). A block-import "skip"
# (BatchVerifyError::Unavailable) is silent, so the real signal that every proof
# was batched is txs_total vs the chain's proof-tx count, not fallback_total.
ON_METRICS="$ARTIFACTS_DIR/metrics-true.txt"
BATCHES="$(metric_sum "$ON_METRICS" batches_total)"
TXS="$(metric_sum "$ON_METRICS" txs_total)"
PROOF_TXS="$(sed -n 's/^proof_txs=//p' "$ARCHIVE_META" 2>/dev/null | head -1)"
PROOF_TXS="${PROOF_TXS:-0}"
AVG="$(awk -v t="$TXS" -v b="$BATCHES" 'BEGIN { if (b>0) printf "%.1f", t/b; else printf "n/a" }')"
printf 'ON batch coverage      : batches=%s txs_total=%s (chain load proof-txs=%s), avg %s txs/batch\n' \
  "$BATCHES" "$TXS" "$PROOF_TXS" "$AVG"
# Aggregate crypto time per batch (duration_seconds). Populated for block import only
# on an instrumented node build; older images record nothing here.
DUR_SUM="$(metric_sumf "$ON_METRICS" duration_seconds_sum)"
DUR_CNT="$(metric_sum "$ON_METRICS" duration_seconds_count)"
if [ "$DUR_CNT" -gt 0 ]; then
  awk -v s="$DUR_SUM" -v c="$DUR_CNT" 'BEGIN {
    printf "ON crypto time         : %.1f ms/batch  (%.3fs total over %d batches)\n", (s/c)*1000, s, c
  }'
else
  echo "ON crypto time         : (duration_seconds not recorded — needs the instrumented node build)"
fi

# --- per-midnight-tx proof verification (the tx-granular OFF-vs-ON signal) ---
# Wall-clock sync time is swamped by peer-connect + AURA cadence + DB writes, so
# compare the ZK crypto directly at transaction granularity via the ledger-side
# `ledger_proof_verify_*` metrics (labelled by mode), which record on BOTH runs:
#   OFF run -> mode="inline"     : per-tx well_formed WITH proofs (cold cache).
#   ON  run -> mode="batch"      : per-aggregate-call crypto (batch_verify_proofs).
#              mode="batch_prep" : per-tx well_formed WITHOUT proofs (non-crypto).
# Per-tx cost = _sum / _txs_total. Adding ON's batch_prep makes the ON figure
# apples-to-apples with OFF's fused well_formed; subtracting it isolates crypto.
OFF_METRICS="$ARTIFACTS_DIR/metrics-false.txt"
PV_DUR=ledger_proof_verify_duration_seconds_sum
PV_TXS=ledger_proof_verify_txs_total
off_inline_sum="$(metric_mode "$OFF_METRICS" "$PV_DUR" inline)"
off_inline_txs="$(metric_mode "$OFF_METRICS" "$PV_TXS" inline)"
on_batch_sum="$(metric_mode "$ON_METRICS" "$PV_DUR" batch)"
on_batch_txs="$(metric_mode "$ON_METRICS" "$PV_TXS" batch)"
on_prep_sum="$(metric_mode "$ON_METRICS" "$PV_DUR" batch_prep)"
on_prep_txs="$(metric_mode "$ON_METRICS" "$PV_TXS" batch_prep)"
echo
echo "--- per-midnight-tx proof verification (crypto, OFF inline vs ON batched) ---"
awk -v ois="$off_inline_sum" -v oit="$off_inline_txs" \
    -v obs="$on_batch_sum"   -v obt="$on_batch_txs" \
    -v ops="$on_prep_sum"    -v opt="$on_prep_txs" 'BEGIN {
  if (oit <= 0 || obt <= 0) {
    print "  (insufficient samples — needs a node built from this branch that records"
    print "   ledger_proof_verify_* on both the OFF and ON runs)"
    exit
  }
  off_tx  = ois / oit;                 # OFF per-tx: well_formed WITH proofs (crypto + non-crypto)
  on_cryp = obs / obt;                 # ON  per-tx: aggregate crypto share
  on_prep = (opt > 0) ? ops / opt : 0; # ON  per-tx: non-crypto well_formed
  on_tx   = on_cryp + on_prep;         # ON  per-tx: full verify, comparable to off_tx
  off_cryp = off_tx - on_prep;         # OFF per-tx crypto (subtract the shared non-crypto cost)
  printf "  OFF inline           : %8.3f ms/tx   (%d txs, well_formed WITH proofs)\n", off_tx*1000, oit
  printf "  ON  batched          : %8.3f ms/tx   (%d txs = %.3f crypto + %.3f prep)\n", on_tx*1000, obt, on_cryp*1000, on_prep*1000
  printf "  full-verify speedup  : %.2fx   (%.3f -> %.3f ms/tx)\n", off_tx/on_tx, off_tx*1000, on_tx*1000
  if (off_cryp > 0 && on_cryp > 0)
    printf "  crypto-only speedup  : %.2fx   (%.3f -> %.3f ms/tx)\n", off_cryp/on_cryp, off_cryp*1000, on_cryp*1000
}'

if [ "$BATCHES" -gt 0 ] && [ "$TXS" -ge "$PROOF_TXS" ]; then
  echo "  ✅ block-import batched every proof-tx (txs_total >= load proof-txs) — no inline fallback"
else
  echo "  ⚠️  txs_total ($TXS) < load proof-txs ($PROOF_TXS): some blocks skipped batching"
  echo "     (BatchVerifyError::Unavailable) and inline-verified — investigate before trusting timing."
fi
echo "  note: fallback_total is mempool-only (always 0 on a syncer); batch size is"
echo "        capped at the funder's DUST-output count, so batches stay small — the"
echo "        per-tx crypto speedup above needs many proof-txs/block to be large."
echo "--- raw ON verify counters ---"
cat "$ON_METRICS" 2>/dev/null || echo "(none scraped)"
echo "--- raw OFF verify counters ---"
cat "$OFF_METRICS" 2>/dev/null || echo "(none scraped)"
echo "═══════════════════════════════════════════════════════════════════"
