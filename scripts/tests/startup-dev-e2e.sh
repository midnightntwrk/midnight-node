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

set -euxo pipefail

# shellcheck disable=SC1091
. "$(dirname "$0")/lib/wait-for-node.sh"

NODE_IMAGE="$1"

if [ -z "$NODE_IMAGE" ]; then
  echo "❌ Missing required argument: NODE_IMAGE"
  echo "Usage: ./startup-dev-e2e.sh ghcr.io/midnight-ntwrk/midnight-node:<tag>"
  exit 1
fi

# Which genesis ledger version to boot against. The CI workflow fans this out
# as a matrix (one job per leg, in parallel); locally, override to test the
# other leg, e.g. `STARTUP_GENESIS=ledger8 just startup-dev-e2e <image>`.
STARTUP_GENESIS="${STARTUP_GENESIS:-ledger9}"

# Extra `docker run` args selecting the genesis. ledger9 is the stock dev
# genesis (no override); ledger8 points at the pinned pre-ledger-9 (v13)
# undeployed fixtures, exercising the genesis_version dispatch path.
GENESIS_ARGS=()
case "$STARTUP_GENESIS" in
  ledger9) ;;
  ledger8)
    GENESIS_ARGS+=(
      -e CHAINSPEC_GENESIS_STATE=res/genesis/genesis_state_undeployed_ledger8.mn
      -e CHAINSPEC_GENESIS_BLOCK=res/genesis/genesis_block_undeployed_ledger8.mn
    )
    ;;
  *)
    echo "❌ Unknown STARTUP_GENESIS='${STARTUP_GENESIS}' (expected ledger9 or ledger8)"
    exit 1
    ;;
esac

echo "🧪 Running Startup E2E test with:"
echo "    NODE_IMAGE=${NODE_IMAGE}"
echo "    STARTUP_GENESIS=${STARTUP_GENESIS}"

# Setup working directory
WORKDIR=$(mktemp -d)
cp -r res "$WORKDIR"

# Create Docker network
docker network create midnight-net-startup || true

# Run the node container
echo "🚀 Launching node container..."
docker run -d --rm \
  --name midnight-node-e2e \
  --network midnight-net-startup \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "${GENESIS_ARGS[@]}" \
  "${NODE_IMAGE}"

# Smoke test: assert finality is advancing, matching the old `finalized #1`
# log-grep semantics this script used to have.
if wait_for_finalized_block http://localhost:9944 1 60; then
    echo "✅ Node started successfully with CFG_PRESET=dev (${STARTUP_GENESIS})"
else
    echo "❌ Node failed to start with CFG_PRESET=dev (${STARTUP_GENESIS})"
    TEST_FAILED=true
fi

# Teardown node
echo "🛑 Cleaning up..."
docker kill midnight-node-e2e || true

# Exit with test result
if [ "${TEST_FAILED:-false}" = true ]; then
  echo "❌ Startup Test failed."
  exit 1
else
  echo "✅ Startup Test complete."
fi
