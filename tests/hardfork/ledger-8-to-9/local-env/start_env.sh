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

# A fresh local-env chain on the ledger-8 release, with the indexer and the Cardano stack.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/lib_local.sh"

echo "ledger-8 release $L8_REF: node $L8_NODE_IMAGE, toolkit $L8_TOOLKIT_IMAGE; indexer $INDEXER_TAG"
require_cmd npm node openssl || exit 3
for img in "$L8_NODE_IMAGE" "$L8_TOOLKIT_IMAGE" "$IMAGE_REGISTRY/chain-indexer:$INDEXER_TAG"; do
    require_image "$img" || exit 3
done

ensure_worktree "$L8_ROOT" "$L8_REF" || cannot_run "cannot create the $L8_REF worktree at $L8_ROOT"
reserve_contracts_ready || cannot_run "cannot check out midnight-reserve-contracts at $RESERVE_CONTRACTS_PATH"
git -C "$L8_ROOT" checkout -q -- local-environment 2>/dev/null || true
nv=$(grep -m1 '^version' "$L8_ROOT/node/Cargo.toml" | sed 's/version *= *"\([^"]*\)".*/\1/')
[ "$nv" = "$L8_EXPECTED_NODE_PREFIX" ] || cannot_run "$L8_REF is node $nv, expected $L8_EXPECTED_NODE_PREFIX"

rm -rf "$STATE_DIR" "$EVIDENCE_DIR" "$DAPPS_DIR"
init_out_dirs

unpin_contract_compiler_apt "$L8_ROOT"
patch_cardano_start "$L8_ROOT"
load_compose_env "$L8_ROOT" "$L8_NODE_IMAGE" "$L8_TOOLKIT_IMAGE"
npm ci --no-audit --no-fund --loglevel=error
bring_up_local_env || cannot_run "local-env did not come up in three attempts"
# The only Cardano block producer must not starve while the host is proving.
docker update --cpu-shares 4096 cardano-node-1 >/dev/null 2>&1 || true

echo "waiting for the first finalized block"
for i in $(seq 1 60); do
    f=$(finalized_height); [ -n "$f" ] && [ "$f" -ge 1 ] && break
    [ "$i" = 60 ] && cannot_run "no finalized block after 5 minutes"
    sleep 5
done
state_set "$STATE_DIR/env.env" LOCAL_ENV_CHECKOUT "$L8_ROOT"
echo "local-env up: node $(node_version), spec $(spec_version), finalized #$(finalized_height)"
