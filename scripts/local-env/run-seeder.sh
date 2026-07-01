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

# Entry point for the `local-env-seed` docker-compose service (built into
# midnight-node-seeder via `earthly +local-env-seeder-image`). Runs ONLY the
# `local_env_seed::seed_wallet` e2e test, which funds the dev wallet (seed
# 0x..01) with NIGHT by driving the cNIGHT->NIGHT bridge end-to-end against the
# live local-env stack (see tests/e2e/tests/local_env_seed.rs).
#
# Idempotent: a `$SEED_MARKER_FILE` in the mounted runtime-values dir short-
# circuits a re-run so a stack restart doesn't double-fund the wallet.

set -euo pipefail

MARKER="${SEED_MARKER_FILE:-/runtime-values/wallets-seeded}"
if [ -f "$MARKER" ]; then
  echo "wallets already seeded ($MARKER present); skipping."
  exit 0
fi

echo "=== local-env wallet seeding: funding dev wallet 0x..01 via c2m bridge ==="

# `depends_on: midnight-node-1 service_started` only guarantees the container
# started, not that its RPC is accepting connections. Wait for the node RPC port
# to open (bash /dev/tcp; no curl/nc in the slim image) before running the test,
# else MidnightClient::new fails with ConnectionRefused.
node_hostport="${E2E_NODE_URL#*://}"
node_host="${node_hostport%%:*}"
node_port="${node_hostport##*:}"
echo "waiting for node RPC at ${node_host}:${node_port} ..."
node_ready=false
for _ in $(seq 1 90); do
  if (exec 3<>"/dev/tcp/${node_host}/${node_port}") 2>/dev/null; then
    exec 3>&- 3<&- 2>/dev/null || true
    node_ready=true
    break
  fi
  sleep 2
done
if [ "$node_ready" != true ]; then
  echo "ERROR: node RPC at ${node_host}:${node_port} not reachable after 180s"
  exit 1
fi
echo "node RPC reachable; starting seeding"

# The seeding test binary is prebuilt + stripped into the image (Earthfile
# +local-env-seeder-image), so we run it directly — no cargo / toolchain / source
# needed at runtime. It is the libtest harness binary; select only our test.
SEEDER_BIN="${SEEDER_BIN:-/usr/local/bin/seeder}"
LOG="$(mktemp)"
set +e
"$SEEDER_BIN" --exact local_env_seed::seed_wallet --nocapture --test-threads=1 2>&1 | tee "$LOG"
status=${PIPESTATUS[0]}
set -e

if [ "$status" -ne 0 ]; then
  echo "ERROR: wallet seeding test failed (exit $status)"
  exit "$status"
fi

# Guard against a mistyped filter silently matching zero tests (which would exit
# 0 having funded nothing). Require the one expected test to have passed.
if ! grep -qE "test result: ok\. 1 passed" "$LOG"; then
  echo "ERROR: seeding test did not run (expected 'test result: ok. 1 passed'); not writing marker"
  exit 1
fi

printf 'seeded %s\n' "$(date -u +%FT%TZ 2>/dev/null || echo now)" > "$MARKER"
echo "=== local-env wallet seeding complete ==="
