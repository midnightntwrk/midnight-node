# shellcheck shell=bash
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

# Shared configuration and helpers for the batch-verification block-import
# performance harness. Sourced by prime.sh and benchmark.sh.
#
# What this harness measures
# ---------------------------
# The PR adds batch ZK-proof verification at block import. Authored blocks skip
# it (they carry StateAction::ApplyChanges), so the path only fires on blocks a
# node *imports* from a peer. The harness therefore:
#   1. (prime.sh)     builds a chain packed with proof-bearing txs on a dev
#                     authority, then archives that node's base_path (tarball).
#   2. (benchmark.sh) restores the archive into a *producer* that just serves
#                     the blocks, then full-syncs a fresh *node-under-test* from
#                     it with BATCH_VERIFY_BLOCK_IMPORT off vs on, timing each.
# The A/B delta isolates the batch-verify speedup (both runs execute the same
# blocks and verify the same proofs; only the batching differs).

# --- resolved paths -------------------------------------------------------
BV_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-$BV_DIR/artifacts}"
ARCHIVE_TAR="${ARCHIVE_TAR:-$ARTIFACTS_DIR/chain-archive.tar.gz}"
ARCHIVE_META="${ARCHIVE_META:-$ARTIFACTS_DIR/chain-archive.meta}"

# rpc-height helpers (wait_for_unfinalized_block, _rpc_get_best_height, ...)
# shellcheck disable=SC1091
. "$BV_DIR/../lib/wait-for-node.sh"

# --- docker / topology ----------------------------------------------------
NETWORK_NAME="${NETWORK_NAME:-batch-verify-net}"
# Chainspec to run. `dev` is the built-in "Midnight Undeployed" spec; a path
# selects a custom one (see the README's "Bigger batches" section).
# CHAIN_OVERRIDDEN records whether the caller chose it, so benchmark.sh can
# default to the spec recorded in the archive meta without overriding an
# explicit choice.
CHAIN_OVERRIDDEN="${CHAIN:+yes}"
CHAIN="${CHAIN:-dev}"
BASE_PATH_IN="${BASE_PATH_IN:-/node/chain}" # matches images/node/Dockerfile BASE_PATH

PRIME_CONTAINER="${PRIME_CONTAINER:-bv-prime}"
PRODUCER_CONTAINER="${PRODUCER_CONTAINER:-bv-producer}"
SYNCER_CONTAINER="${SYNCER_CONTAINER:-bv-syncer}"

PRIME_VOLUME="${PRIME_VOLUME:-bv-prime-data}"
PRODUCER_VOLUME="${PRODUCER_VOLUME:-bv-producer-data}"
SYNCER_VOLUME="${SYNCER_VOLUME:-bv-syncer-data}"

# The producer keeps the dev preset's node key (0000..0001), so its peer id is
# stable and usable as a bootnode. The syncer generates a fresh random node key
# per run (see benchmark.sh) so rapid reconnect cycles don't collide.
P2P_PORT="${P2P_PORT:-30333}"
# Container RPC is always 9944 and prometheus 9615; these are the host mappings.
PRODUCER_RPC_HOST_PORT="${PRODUCER_RPC_HOST_PORT:-9945}"
SYNCER_RPC_HOST_PORT="${SYNCER_RPC_HOST_PORT:-9944}"
SYNCER_PROM_HOST_PORT="${SYNCER_PROM_HOST_PORT:-9615}"

# --- local (host-process) mode -------------------------------------------
# When benchmark.sh is invoked WITHOUT a node image it runs the node from a
# locally-built binary as host processes instead of containers (see the mode
# split in benchmark.sh, mirroring scripts/tests/toolkit-tokens-minter-e2e.sh).
# The image's base is amazonlinux 2023 (glibc 2.34); a binary built on a newer
# host won't run inside it, so local mode never containerises the local binary —
# it runs it directly on the host, where its own glibc is available.
#
# Repo root (…/scripts/tests/batch-verify-perf → three up). Needed because the
# `dev` preset (res/cfg/dev.toml) references its chainspec/genesis/mock files by
# RELATIVE path, so the local binary must run with the repo root as its CWD.
REPO_ROOT="${REPO_ROOT:-$(cd "$BV_DIR/../../.." && pwd)}"
# Locally-built node binary used in local mode. Release by default (this is a
# perf harness); override with NODE_BIN=… (e.g. target/debug/midnight-node) when
# you only need to check that the metrics appear.
NODE_BIN="${NODE_BIN:-$REPO_ROOT/target/release/midnight-node}"
# Fixed dev-preset node key → stable producer peer id (matches res/cfg/dev.toml).
DEV_NODE_KEY="${DEV_NODE_KEY:-0000000000000000000000000000000000000000000000000000000000000001}"
# Host base-paths + logs for the two local nodes (kept under artifacts for debugging).
LOCAL_WORK_DIR="${LOCAL_WORK_DIR:-$ARTIFACTS_DIR/local}"
PRODUCER_DIR="${PRODUCER_DIR:-$LOCAL_WORK_DIR/producer}"
SYNCER_DIR="${SYNCER_DIR:-$LOCAL_WORK_DIR/syncer}"
PRODUCER_LOG="${PRODUCER_LOG:-$LOCAL_WORK_DIR/producer.log}"
SYNCER_LOG="${SYNCER_LOG:-$LOCAL_WORK_DIR/syncer.log}"
# Distinct host ports so producer and syncer processes never collide. RPC/prom
# reuse the docker host mappings (so downstream URLs are identical); only the P2P
# ports are local-mode-only.
PRODUCER_PROM_PORT="${PRODUCER_PROM_PORT:-9616}" # producer prom (unused; kept off 9615)
PRODUCER_P2P_PORT="${PRODUCER_P2P_PORT:-30334}"
SYNCER_P2P_PORT="${SYNCER_P2P_PORT:-30335}"

# --- prime workload tunables ----------------------------------------------
# The proof-heavy chain is built in two steps (see prime.sh):
#   1. fan-out: one-or-more `single-tx` calls split the genesis shielded balance
#      into LOAD_TXS distinct coins, one per derived wallet ("lots of outputs").
#   2. load:    `batch-single-tx` builds LOAD_TXS *independent* shielded
#      transfers -- source_seed = a distinct funded wallet (its own coin, so no
#      coin-selection contention), funding_seed = the single genesis DUST wallet
#      (DUST is contention-free, so one funder covers every fee). Independent
#      txs pack blocks, and each carries a zswap proof -- the work batch
#      verification accelerates.
# Scale LOAD_TXS for more proofs (=> longer prime; every tx is proved up-front).
GENESIS_SEED="${GENESIS_SEED:-0000000000000000000000000000000000000000000000000000000000000001}"
NETWORK_ID="${NETWORK_ID:-undeployed}"     # dev chain = "Midnight Undeployed"
LOAD_TXS="${LOAD_TXS:-60}"                 # number of independent proof-bearing txs
LOAD_CHUNK="${LOAD_CHUNK:-5}"              # txs per batch-single-tx invocation. Capped by the
                                           # genesis funder's DUST-*output* count: batch-single-tx
                                           # builds all specs against one snapshot without reserving
                                           # DUST between them, so each concurrent build needs a
                                           # distinct DUST output. Genesis starts with 5. Re-fetching
                                           # between chunks picks up the change outputs, so the loop
                                           # can run indefinitely -- balance is never the limit.
FANOUT_CHUNK="${FANOUT_CHUNK:-25}"         # outputs per fan-out single-tx (tx-size cap)
FAN_AMOUNT="${FAN_AMOUNT:-100}"            # shielded coin value seeded into each wallet
SEND_AMOUNT="${SEND_AMOUNT:-100}"          # shielded amount each load tx moves (<= FAN_AMOUNT)
LOAD_RATE="${LOAD_RATE:-20}"               # submit rate for the load (txs/sec)
SEED_BASE="${SEED_BASE:-65536}"            # derived source seeds = SEED_BASE+1 .. SEED_BASE+LOAD_TXS
SHIELDED="${SHIELDED:-1}"                  # 1 => shielded (proofs); 0 => unshielded (no proofs)
# Persistent toolkit cache volume (ZK params + fetch/ledger cache) shared across all toolkit
# invocations so the ~tens-of-MB proving keys download once, not per chunk.
TOOLKIT_CACHE_VOLUME="${TOOLKIT_CACHE_VOLUME:-bv-toolkit-cache}"
# Default target height if the meta file is missing; prime.sh records the real one.
TARGET_HEIGHT="${TARGET_HEIGHT:-10}"

# --- benchmark tunables ---------------------------------------------------
SYNC_TIMEOUT_SECS="${SYNC_TIMEOUT_SECS:-1800}"
STALL_TIMEOUT_SECS="${STALL_TIMEOUT_SECS:-240}"
POLL_INTERVAL_SECS="${POLL_INTERVAL_SECS:-2}"
# How often the benchmark checks whether the syncer has reached the target height. This is the
# *measurement* resolution: a sync that truly takes T seconds is reported somewhere in
# [T, T + interval), because completion is only noticed on the next poll. At the old 2 s it
# quantised a ~4 s sync into "4.1 s or 6.2 s" and buried the sub-second effect the benchmark
# exists to measure -- more repeats do not help, since the error is a floor-to-poll-boundary
# artifact rather than zero-mean noise. Each poll is one cheap local RPC.
SYNC_POLL_INTERVAL_SECS="${SYNC_POLL_INTERVAL_SECS:-0.1}"
# Optional batch-verify tuning, forwarded to the syncer if set in the env:
#   BATCH_VERIFY_MAX_BATCH_SIZE BATCH_VERIFY_TARGET_BATCH_SIZE
#   BATCH_VERIFY_MAX_AGE_MS BATCH_VERIFY_WORKERS BATCH_VERIFY_QUEUE_CAPACITY

# --- helpers --------------------------------------------------------------
log() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*" >&2; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

require_cmds() {
  local cmds=("$@") c
  [ "${#cmds[@]}" -eq 0 ] && cmds=(docker curl)
  for c in "${cmds[@]}"; do
    command -v "$c" >/dev/null 2>&1 || die "missing required command: $c"
  done
}

ensure_network() {
  docker network inspect "$NETWORK_NAME" >/dev/null 2>&1 \
    || docker network create "$NETWORK_NAME" >/dev/null
}

rm_container() { docker rm -f "$1" >/dev/null 2>&1 || true; }
rm_volume()    { docker volume rm -f "$1" >/dev/null 2>&1 || true; }

# best-block height (decimal) from an http rpc url, empty on error
best_height() { _rpc_get_best_height "$1"; }

# Chain-selection args for an *authoring* node.
#
# `--dev` is a shorthand that also injects Alice and force-authoring, but it pins the chain id to
# "dev", and `Cfg::load_spec` maps that to a hardcoded built-in spec — `chainspec_genesis_state`
# and friends are validated but never consulted. So a custom CHAIN (a chainspec JSON path, e.g. one
# built with more genesis DUST outputs) cannot go through `--dev`; the flags it implies have to be
# spelled out instead. Non-authoring nodes just take `--chain "$CHAIN"` directly.
authoring_chain_args() {
  if [ "$CHAIN" = "dev" ]; then
    printf -- '--dev\n'
  else
    printf -- '--chain\n%s\n--alice\n--force-authoring\n' "$CHAIN"
  fi
}


# Milliseconds since the epoch. `date +%s%3N` is GNU-only: BSD/macOS `date` has no `%N` and emits a
# literal "N", which collapses the millisecond subtraction to ~0 and silently reports every sync as
# instantaneous. Prefer GNU date when present, else Python.
now_ms() {
  if date +%s%3N 2>/dev/null | grep -qE '^[0-9]+$'; then
    date +%s%3N
  else
    python3 -c 'import time; print(int(time.time() * 1000))'
  fi
}


# archive_volume <volume> <host-tar.gz>: gzip the contents of a named volume.
# Uses busybox (piping through gzip so we do not rely on busybox tar's -z).
archive_volume() {
  local vol="$1" out="$2"
  mkdir -p "$(dirname "$out")"
  docker run --rm \
    -v "$vol":/data \
    -v "$(dirname "$out")":/out \
    busybox sh -c "cd /data && tar cf - . | gzip -1 -c > /out/$(basename "$out")"
}

# restore_volume <volume> <host-tar.gz>: recreate the volume and extract into it.
restore_volume() {
  local vol="$1" tar="$2"
  [ -f "$tar" ] || die "archive not found: $tar (run prime.sh first)"
  rm_volume "$vol"
  docker volume create "$vol" >/dev/null
  docker run --rm \
    -v "$vol":/data \
    -v "$(dirname "$tar")":/in \
    busybox sh -c "cd /data && gzip -dc /in/$(basename "$tar") | tar xf -"
}

# host_restore_dir <dir> <host-tar.gz>: the local-mode analogue of restore_volume —
# recreate a host directory and extract the archived base_path into it (the tarball
# root is the base_path contents, i.e. chains/…). Uses GNU tar's -z.
host_restore_dir() {
  local dir="$1" tar="$2"
  [ -f "$tar" ] || die "archive not found: $tar (run prime.sh first)"
  rm -rf "$dir"
  mkdir -p "$dir"
  tar xzf "$tar" -C "$dir"
}

# Scrape the verification counters (summary lines only, dropping histogram
# buckets and HELP/TYPE comments). Captures two metric families:
#   - `midnight_batch_verify_*` : node-side batch coverage/counters (ON only).
#   - `ledger_proof_verify_*`   : ledger-side per-tx crypto time, labelled by
#     `mode` (inline / batch / batch_prep) — present on both the OFF and ON runs,
#     so the two paths can be compared at midnight-transaction granularity.
# The metrics carry labels
# (e.g. `ledger_proof_verify_duration_seconds_sum{mode="inline",chain="undeployed"} 0.1`),
# so match the name prefix, not a name-then-space.
# Scrape the counters the report is derived from.
#
# Deliberately wider than the batch-verify families: proof verification is only about a quarter of
# sync time, and the rest was invisible because this filter dropped it. `ledger_post_block_update_`
# and `storage_flush_time` cover the work `pallet_midnight`'s `on_finalize` does on EVERY block
# (ledger bookkeeping, then persisting the state), which is paid whether or not the block carried
# transactions -- on a chain of mostly-empty blocks that is the prime suspect for the remainder.
# `substrate_block_verification_and_import_time` is the import pipeline's own view, to check the
# ledger-side numbers against the total rather than assuming they explain it.
scrape_batch_metrics() {
  local prom_url="$1"
  curl -sf --max-time 3 "$prom_url" 2>/dev/null \
    | grep -E '^(midnight_batch_verify_|ledger_proof_verify_|ledger_post_block_update_|storage_(fetch|flush)_time|ledger_txs_(processing|validating)_time|substrate_block_verification_and_import_time)' \
    | grep -vE '_bucket|^#' || true
}

# Sum the values of a batch-verify metric across its label sets, from a scraped
# metrics file. e.g. metric_sum <file> batches_total -> total across outcomes.
# Integer form (for counters); use metric_sumf for float metrics like durations.
metric_sum() {
  local file="$1" name="$2"
  awk -v n="midnight_batch_verify_${name}" '
    $1 ~ "^"n"([{ ]|$)" { s += $NF } END { printf "%d", s+0 }' "$file" 2>/dev/null
}

# Float form, for e.g. duration_seconds_sum.
metric_sumf() {
  local file="$1" name="$2"
  awk -v n="midnight_batch_verify_${name}" '
    $1 ~ "^"n"([{ ]|$)" { s += $NF } END { printf "%.6f", s+0 }' "$file" 2>/dev/null
}

# Sum a `mode`-labelled ledger metric across its remaining label sets.
#   metric_mode <file> <full-metric-name> <mode>
# e.g. metric_mode metrics-false.txt ledger_proof_verify_duration_seconds_sum inline
# Matches the exact metric name at the start of the token and the `mode="…"` label
# anywhere in the line, so it is robust to other labels (chain=…) and their order.
metric_mode() {
  local file="$1" name="$2" mode="$3"
  awk -v n="$name" -v m="mode=\"$mode\"" '
    index($1, n) == 1 && index($0, m) > 0 { s += $NF } END { printf "%.6f", s + 0 }' "$file" 2>/dev/null
}

# gen_seeds <count> <base>: print <count> distinct 32-byte hex seeds
# (base+1 .. base+count), one per line. The base keeps them clear of the
# low reserved dev seeds (genesis is 0000..0001).
gen_seeds() {
  local n="$1" base="$2" i
  for (( i = 1; i <= n; i++ )); do
    printf '%064x\n' "$(( base + i ))"
  done
}

# derive_addrs <toolkit_image> <addr-flag> <seed...>: print the address of the
# given kind (addr-flag = --shielded | --unshielded) for each seed, in order,
# one per line. Runs a single container (entrypoint overridden to a shell loop)
# so we don't pay container startup per seed. show-address is pure key
# derivation -- no node, no file I/O -- so running the binary as root is fine.
derive_addrs() {
  local image="$1" flag="$2"; shift 2
  docker run --rm --entrypoint sh "$image" -c '
    net="$1"; flag="$2"; shift 2
    for s in "$@"; do
      /midnight-node-toolkit show-address --network "$net" "$flag" --seed "$s"
    done
  ' _ "$NETWORK_ID" "$flag" "$@"
}

# --- paired A/B statistics (shared by benchmark.sh and mempool-benchmark.sh) ---
#
# Both harnesses interleave their arms, so sample i of each arm comes from the same repeat and
# their difference is immune to the machine-wide drift that dominates the raw spread. Comparing
# the two arms' spreads instead throws the pairing away and calls unanimous data "unresolved".

# median, min and mean of a list of floats. $1 is the unit suffix to print.
stats_unit() {
  local unit="$1"; shift
  printf '%s\n' "$@" | sort -n | awk -v u="$unit" '
    {v[NR]=$1; s+=$1}
    END {
      m = (NR % 2) ? v[(NR+1)/2] : (v[NR/2] + v[NR/2+1]) / 2
      printf "median=%.2f%s min=%.2f%s mean=%.2f%s", m, u, v[1], u, s/NR, u
    }'
}
stats() { stats_unit s "$@"; }
median() { printf '%s\n' "$@" | sort -n | awk '{v[NR]=$1} END {printf "%.2f", (NR%2) ? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2}'; }
# Spread of the samples, kept for diagnostics only -- never as the resolution test.
spread() { printf '%s\n' "$@" | sort -n | awk '{v[NR]=$1} END {printf "%.2f", v[NR]-v[1]}'; }

# Prints the paired deltas, their median/mean, and a one-sided sign test.
# Usage: report_paired <unit> <n> <off...> <on...>   (both arms must have n samples)
report_paired() {
  local unit="$1" n="$2"; shift 2
  local off=( "${@:1:$n}" ) on=( "${@:$((n+1)):$n}" )
  printf 'paired deltas (off-on) : [%s]\n' "$(
    local i
    for i in $(seq 0 $(( n - 1 ))); do
      awk -v a="${off[$i]}" -v b="${on[$i]}" 'BEGIN{printf "%+.1f ", a-b}'
    done)"
  local deltas=() i
  for i in $(seq 0 $(( n - 1 ))); do
    deltas+=( "$(awk -v a="${off[$i]}" -v b="${on[$i]}" 'BEGIN{printf "%.4f", a-b}')" )
  done
  # Sorted for the median; sign counts come from the unsorted list. (macOS awk has no asort.)
  printf '%s\n' "${deltas[@]}" | sort -n | awk -v u="$unit" '
    {v[NR]=$1; s+=$1}
    END {
      med = (NR % 2) ? v[(NR+1)/2] : (v[NR/2] + v[NR/2+1]) / 2
      mean = s/NR
      printf "  median paired delta  : %+.2f%s   (mean %+.2f%s)\n", med, u, mean, u
      # A run far from the median is usually a harness flake (a slow peer connect, a stalled
      # submission), and it lands on whichever arm happened to be running -- so trust the median.
      d = mean - med; if (d < 0) d = -d
      if (d > 1) {
        print "  ⚠️  one or more runs are far from the median (likely a harness flake);"
        print "      the mean above is not meaningful — read the median and the sample list."
      }
    }'
  printf '%s\n' "${deltas[@]}" | awk '
    {if ($1 > 0) wins++; else if ($1 < 0) losses++}
    END {
      k = (wins > losses) ? wins : losses
      # Sign test, one-sided: P(X >= k | p=0.5) = 2^-NR * sum_{j=k..NR} C(NR,j).
      tail = 0; c = 1
      for (j = NR; j >= k; j--) { tail += c; c = c * j / (NR - j + 1) }
      half = 1; for (i = 0; i < NR; i++) half = half / 2
      p = tail * half
      printf "  ON faster in %d/%d pairs  (sign test p = %.3f, one-sided)\n", wins+0, NR, p
      # Judge by the sign test, not by unanimity: with enough pairs a few disagreements are
      # expected and the result is still decisive, while 3/3 agreeing establishes very little.
      if (p > 0.05) {
        print "  ⚠️  not resolved (p > 0.05): these pairs do not establish a direction. Raise"
        print "      REPEATS, or use a workload where the effect is a larger share of the total."
      }
    }'
}
