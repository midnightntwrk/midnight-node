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
TOOLKIT_IMAGE="$2"

echo "🎯 Running Toolkit E2E test"
echo "🧱 NODE_IMAGE: $NODE_IMAGE"
echo "🧱 TOOLKIT_IMAGE: $TOOLKIT_IMAGE"

# Ensure Docker network exists
docker network create ledger-params-e2e-net || true

# Start node in background (without --rm so we can get logs on failure)
echo "🚀 Starting node container..."
docker run -d \
  --name midnight-node \
  --network ledger-params-e2e-net \
  -p 9944:9944 \
  -e CFG_PRESET=dev \
  -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
  "$NODE_IMAGE"

cleanup() {
    echo "🛑 Cleaning up..."
    # Show logs if container exists (helpful for debugging crashes)
    if docker container inspect midnight-node &>/dev/null; then
        echo "📋 Node container logs:"
        docker logs midnight-node --tail 100 || true
    fi
    docker container stop midnight-node || true
    docker container rm midnight-node || true
    docker network rm ledger-params-e2e-net || true
}
# --- Always-cleanup: runs on success, error, or interrupt ---
trap cleanup EXIT

if ! wait_for_unfinalized_block http://localhost:9944 2; then
    echo "📋 Container status:"
    docker container inspect midnight-node --format '{{.State.Status}} - Exit code: {{.State.ExitCode}}' || true
    echo "📋 Container logs:"
    docker logs midnight-node || true
    exit 1
fi

# Run toolkit commands
echo "📦 Running toolkit tests..."

echo "Get version for toolkit"
docker run --rm -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" version

current_parameters=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" \
        show-ledger-parameters -r ws://midnight-node:9944 --serialize
)

docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" \
    update-ledger-parameters -r ws://midnight-node:9944 -t //Alice -t //Bob -c //Dave -c //Eve --c-to-m-bridge-min-amount 2000

new_parameters=$(
    docker run --rm -e RESTORE_OWNER="$(id -u):$(id -g)" -e RUST_BACKTRACE=1 --network ledger-params-e2e-net "$TOOLKIT_IMAGE" \
        show-ledger-parameters -r ws://midnight-node:9944 --serialize
)

if [ "$current_parameters" != "$new_parameters" ]; then
  echo "✅ Ledger parameters update successful"
else
  echo "❌ Ledger parameters update failed"
  exit 1
fi

echo "✅ Toolkit E2E"
