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

# Switching `storage_separation` from `separate` to `unified` used to mean
# resyncing from genesis, even though the node already had every ledger node on
# disk — just in a database of its own. It now folds that database into the
# shared one on start-up instead.
#
# This test walks an operator through the switch: build up real chain data in
# `separate` mode, stop, flip the config, start again on the same data
# directory, and require that the chain carries on from where it left off with
# byte-identical ledger state.

set -euxo pipefail

# shellcheck disable=SC1091
. "$(dirname "$0")/lib/wait-for-node.sh"

NODE_IMAGE="${1:-}"

if [ -z "$NODE_IMAGE" ]; then
  echo "❌ Missing required argument: NODE_IMAGE"
  echo "Usage: ./storage-separation-migration-e2e.sh ghcr.io/midnight-ntwrk/midnight-node:<tag>"
  exit 1
fi

# How far to get in `separate` mode before switching, and how much further the
# node has to go after each restart to count as healthy.
BLOCKS_BEFORE_SWITCH="${BLOCKS_BEFORE_SWITCH:-10}"
BLOCKS_AFTER_SWITCH="${BLOCKS_AFTER_SWITCH:-5}"
# 6s blocks, plus room for start-up and the migration itself.
BLOCK_TIMEOUT="${BLOCK_TIMEOUT:-240}"

CONTAINER=midnight-node-storage-separation-e2e
RPC_URL=http://localhost:9944
MIGRATION_LOG_LINE="Migrating ledger storage into the unified database"

echo "🧪 Running storage_separation migration E2E test with:"
echo "    NODE_IMAGE=${NODE_IMAGE}"

WORKDIR=$(mktemp -d)
DATA_DIR="${WORKDIR}/data"
mkdir -p "$DATA_DIR"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

# `--user` keeps the chain data owned by the invoking user, so this script can
# inspect it between runs and clean it up afterwards.
start_node() {
  local separation="$1"
  docker run -d \
    --name "$CONTAINER" \
    --user "$(id -u):$(id -g)" \
    -p 9944:9944 \
    -v "${DATA_DIR}:/data" \
    -e BASE_PATH=/data \
    -e CFG_PRESET=dev \
    -e STORAGE_SEPARATION="$separation" \
    -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
    "$NODE_IMAGE"
}

# Graceful stop: the node has to flush ledger storage before the migration can
# pick it up, which is exactly what an operator following the upgrade notes does.
stop_node() {
  capture_logs "$1"
  docker stop -t 30 "$CONTAINER"
  capture_logs "$1"
  docker rm -f "$CONTAINER" >/dev/null
}

# Snapshots this run's logs and prints the file it wrote them to. A file rather
# than a pipe because `grep -q` closes the pipe on its first match, which
# `pipefail` then reports as a failed pipeline. Each phase gets its own label so
# an assertion can never be satisfied by a line from an earlier run.
capture_logs() {
  local log="${WORKDIR}/$1.log"
  docker logs "$CONTAINER" > "$log" 2>&1 || true
  echo "$log"
}

fail() {
  set +x
  echo "❌ $1"
  if docker inspect "$CONTAINER" >/dev/null 2>&1; then
    capture_logs running >/dev/null
  fi
  for log in "${WORKDIR}"/*.log; do
    [ -f "$log" ] || continue
    echo "--- last 60 lines of $(basename "$log") ---"
    tail -60 "$log"
  done
  exit 1
}

# rpc <method> [params-json]: prints the `result` member, fails on a JSON-RPC error.
rpc() {
  local method="$1"
  local params="${2:-[]}"
  local response
  response=$(curl -sf --max-time 10 -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params},\"id\":1}" \
    "$RPC_URL")
  if echo "$response" | jq -e 'has("error")' >/dev/null; then
    echo "❌ RPC ${method} failed: ${response}" >&2
    return 1
  fi
  echo "$response" | jq -c '.result'
}

best_height() {
  rpc chain_getHeader | jq -r '.number' | xargs printf '%d\n'
}

# --- phase 1: build up chain data in `separate` mode -----------------------

echo "🚀 Starting node with storage_separation=separate..."
start_node separate

if ! wait_for_unfinalized_block "$RPC_URL" "$BLOCKS_BEFORE_SWITCH" "$BLOCK_TIMEOUT"; then
  fail "node did not reach block ${BLOCKS_BEFORE_SWITCH} in separate mode"
fi

# Pin the assertions to a block that exists before the switch, so they compare
# the same chain and the same ledger state on both sides of the migration.
CHECKPOINT_HASH=$(rpc chain_getBlockHash "[${BLOCKS_BEFORE_SWITCH}]" | jq -r '.')
LEDGER_ROOT_BEFORE=$(rpc midnight_ledgerStateRoot "[\"${CHECKPOINT_HASH}\"]")
echo "📌 block ${BLOCKS_BEFORE_SWITCH} = ${CHECKPOINT_HASH}"

case "$LEDGER_ROOT_BEFORE" in
  '' | null | '[]') fail "could not read the ledger state root in separate mode" ;;
esac

echo "🛑 Stopping node..."
stop_node separate

# Sanity-check the starting point: this is what the migration reads from.
[ -f "${DATA_DIR}/ledger_storage/metadata" ] ||
  fail "separate mode should have created ${DATA_DIR}/ledger_storage"

# --- phase 2: restart the same data directory in `unified` mode ------------

echo "🚀 Restarting the same data directory with storage_separation=unified..."
start_node unified

# First that the migration finished and the node came back up at all, then that
# it goes on to author genuinely new blocks from wherever it resumed.
if ! wait_for_unfinalized_block "$RPC_URL" "$BLOCKS_BEFORE_SWITCH" "$BLOCK_TIMEOUT"; then
  fail "migrated node did not come back up"
fi
RESUMED_HEIGHT=$(best_height)
if ! wait_for_unfinalized_block "$RPC_URL" "$((RESUMED_HEIGHT + BLOCKS_AFTER_SWITCH))" "$BLOCK_TIMEOUT"; then
  fail "migrated node did not produce ${BLOCKS_AFTER_SWITCH} further blocks from ${RESUMED_HEIGHT}"
fi

# The chain continued rather than starting over.
CHECKPOINT_HASH_AFTER=$(rpc chain_getBlockHash "[${BLOCKS_BEFORE_SWITCH}]" | jq -r '.')
[ "$CHECKPOINT_HASH_AFTER" = "$CHECKPOINT_HASH" ] ||
  fail "block ${BLOCKS_BEFORE_SWITCH} changed across the migration: ${CHECKPOINT_HASH} -> ${CHECKPOINT_HASH_AFTER}"

# And the ledger state behind it came across intact. A resync from genesis, or a
# half-copied ledger, would not reproduce this root.
LEDGER_ROOT_AFTER=$(rpc midnight_ledgerStateRoot "[\"${CHECKPOINT_HASH}\"]")
[ "$LEDGER_ROOT_AFTER" = "$LEDGER_ROOT_BEFORE" ] ||
  fail "ledger state root at block ${BLOCKS_BEFORE_SWITCH} changed across the migration: ${LEDGER_ROOT_BEFORE} -> ${LEDGER_ROOT_AFTER}"

# The migration ran, and cleaned up after itself.
grep -q "$MIGRATION_LOG_LINE" "$(capture_logs migrated)" ||
  fail "expected the node to log a ledger storage migration"
[ ! -e "${DATA_DIR}/ledger_storage" ] ||
  fail "${DATA_DIR}/ledger_storage should have been retired after the migration"
[ -d "${DATA_DIR}/ledger_storage.migrated" ] ||
  fail "expected the migrated-from database at ${DATA_DIR}/ledger_storage.migrated"
[ ! -e "${DATA_DIR}/ledger_storage.importing" ] ||
  fail "the in-progress marker should have been removed"

echo "🛑 Stopping node..."
stop_node unified

# --- phase 3: the migration does not run a second time ---------------------

echo "🚀 Restarting once more in unified mode..."
start_node unified

if ! wait_for_unfinalized_block "$RPC_URL" "$BLOCKS_BEFORE_SWITCH" "$BLOCK_TIMEOUT"; then
  fail "node failed to restart from the migrated database"
fi
RESUMED_HEIGHT=$(best_height)
if ! wait_for_unfinalized_block "$RPC_URL" "$((RESUMED_HEIGHT + BLOCKS_AFTER_SWITCH))" "$BLOCK_TIMEOUT"; then
  fail "node did not keep producing blocks after restarting on the migrated database"
fi

if grep -q "$MIGRATION_LOG_LINE" "$(capture_logs restarted)"; then
  fail "the migration should only run once"
fi

echo "✅ storage_separation migration E2E test complete."
