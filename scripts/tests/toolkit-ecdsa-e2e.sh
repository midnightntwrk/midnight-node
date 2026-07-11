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

# End-to-end coverage for ledger-9 ECDSA unshielded-signature support in the toolkit
# (https://github.com/midnightntwrk/midnight-node/issues/1542). Runs against a `dev` node,
# whose genesis is built on ledger 9, so the ECDSA scheme is accepted on-chain.
#
# What it proves against a live node (the ledger runs `Transaction::well_formed`, i.e. the
# real `signature_verify`, on every submitted tx):
#   1. ECDSA unshielded address derivation is wired and distinct from Schnorr for the same seed.
#   2. A contract can be deployed with an ECDSA contract-maintenance committee.
#   3. A maintenance update signed by an ECDSA committee is accepted (ECDSA-only signing).
#   4. A maintenance update signed by a mixed Schnorr+ECDSA committee is accepted
#      (per-member scheme dispatch), and authority rotations persist across sequential updates.

set -euxo pipefail

# shellcheck disable=SC1091
. "$(dirname "$0")/lib/wait-for-node.sh"

NODE_IMAGE="$1"
TOOLKIT_IMAGE="$2"
RNG_SEED="0000000000000000000000000000000000000000000000000000000000000037"

# Committee members only ever sign maintenance updates, so they need no on-chain funds. Fees are
# paid by the default (Schnorr) funding seed. Keep every seed distinct so the toolkit's shared
# cross-scheme guard never sees one seed requested under two schemes in a single invocation.
ECDSA_AUTH_1="1000000000000000000000000000000000000000000000000000000000000001"
SCHNORR_AUTH_2="2000000000000000000000000000000000000000000000000000000000000002"
ECDSA_AUTH_3="3000000000000000000000000000000000000000000000000000000000000003"

echo "🎯 Running Toolkit ECDSA E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Ensure Docker network exists
docker network create toolkit-ecdsa-e2e-net || true

# Start node in background
echo "🚀 Starting node container..."
docker run -d --rm \
  --name midnight-node-ecdsa \
  --network toolkit-ecdsa-e2e-net \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  "$NODE_IMAGE"

tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t 'ecdsae2e')
cleanup() {
    echo "🛑 Killing node container..."
    docker container stop midnight-node-ecdsa
    echo "🧹 Removing tempdir..."
    rm -rf "$tempdir"
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

wait_for_unfinalized_block http://localhost:9944 2

echo "📦 Running toolkit ECDSA tests..."

# ---------------------------------------------------------------------------
# 1. ECDSA unshielded address derivation (acceptance criterion #2)
# ---------------------------------------------------------------------------
echo "🔑 Deriving Schnorr and ECDSA unshielded addresses for the same seed..."
schnorr_address=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-address \
    --network undeployed \
    --seed "$ECDSA_AUTH_1" \
    --unshielded
)
ecdsa_address=$(
    docker run --rm -e RUST_BACKTRACE=1 "$TOOLKIT_IMAGE" \
    show-address \
    --network undeployed \
    --seed "ecdsa:$ECDSA_AUTH_1" \
    --unshielded
)

echo "Schnorr unshielded address: $schnorr_address"
echo "ECDSA   unshielded address: $ecdsa_address"

# Same seed, different scheme => different NIGHT identity, hence a different address.
if [ "$schnorr_address" = "$ecdsa_address" ]; then
    echo "❌ Error: ECDSA and Schnorr addresses must differ for the same seed"
    exit 1
fi
case "$ecdsa_address" in
    mn_addr*) ;;
    *) echo "❌ Error: unexpected ECDSA unshielded address HRP: $ecdsa_address"; exit 1 ;;
esac

# ---------------------------------------------------------------------------
# 2. Deploy a contract with an ECDSA maintenance committee (acceptance criterion #3)
# ---------------------------------------------------------------------------
deploy_filename="ecdsa_contract_deploy.mn"

echo "🚀 Deploying contract-simple with an ECDSA maintenance committee..."
docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 \
    -v "$tempdir":/out --network toolkit-ecdsa-e2e-net "$TOOLKIT_IMAGE" generate-txs \
    --dest-file "/out/$deploy_filename" \
    contract-simple deploy \
    --rng-seed "$RNG_SEED" \
    --authority-seed "ecdsa:$ECDSA_AUTH_1" \
    -s ws://midnight-node-ecdsa:9944

contract_address=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 \
        -v "$tempdir":/out "$TOOLKIT_IMAGE" \
        contract-address --src-file "/out/$deploy_filename"
)
echo "Deployed contract address: $contract_address"

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 \
    -v "$tempdir":/out --network toolkit-ecdsa-e2e-net "$TOOLKIT_IMAGE" generate-txs \
    --src-file="/out/$deploy_filename" send \
    -d ws://midnight-node-ecdsa:9944

# ---------------------------------------------------------------------------
# 3. Maintenance signed by the ECDSA committee -> rotate to a mixed committee
#    (acceptance criterion #3: ECDSA-only signing accepted on-chain)
#    Initial authority counter is 0, so this update uses --counter 0.
# ---------------------------------------------------------------------------
echo "🔧 Maintenance #1: ECDSA authority rotates to a mixed Schnorr+ECDSA committee..."
docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 \
    --network toolkit-ecdsa-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address" \
    --counter 0 \
    --authority-seed "ecdsa:$ECDSA_AUTH_1" \
    --new-authority-seed "ecdsa:$ECDSA_AUTH_1" \
    --new-authority-seed "schnorr:$SCHNORR_AUTH_2" \
    --threshold 2 \
    -s ws://midnight-node-ecdsa:9944 \
    -d ws://midnight-node-ecdsa:9944

# ---------------------------------------------------------------------------
# 4. Maintenance signed by the MIXED committee -> rotate to a fresh ECDSA committee
#    (acceptance criterion #3: per-member scheme dispatch; a single update carrying both an
#     ECDSA and a Schnorr signature is accepted). The previous rotation bumped the authority
#     counter to 1, so this update uses --counter 1.
# ---------------------------------------------------------------------------
echo "🔧 Maintenance #2: mixed Schnorr+ECDSA authority rotates to a fresh ECDSA committee..."
docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 \
    --network toolkit-ecdsa-e2e-net "$TOOLKIT_IMAGE" \
    generate-txs contract-simple maintenance \
    --rng-seed "$RNG_SEED" \
    --contract-address "$contract_address" \
    --counter 1 \
    --authority-seed "ecdsa:$ECDSA_AUTH_1" \
    --authority-seed "schnorr:$SCHNORR_AUTH_2" \
    --new-authority-seed "ecdsa:$ECDSA_AUTH_3" \
    -s ws://midnight-node-ecdsa:9944 \
    -d ws://midnight-node-ecdsa:9944

echo "✅ Toolkit ECDSA E2E"
