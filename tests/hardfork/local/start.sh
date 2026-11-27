#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Operator step on local-env: a fresh chain on the old release, with the indexer and the
# Cardano stack. Everything the suite recorded on an earlier chain is deleted.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/local_env.sh"

echo "old release $OLD_REF: node $OLD_NODE_IMAGE, toolkit $OLD_TOOLKIT_IMAGE; indexer $INDEXER_TAG"
require_cmd npm node openssl gh || exit 3
for image in "$OLD_NODE_IMAGE" "$OLD_TOOLKIT_IMAGE" "$IMAGE_REGISTRY/chain-indexer:$INDEXER_TAG"; do
    require_image "$image" || exit 3
done

ensure_worktree "$OLD_ROOT" "$OLD_REF" || { echo "cannot create the $OLD_REF worktree at $OLD_ROOT" >&2; exit 3; }
reserve_contracts_ready || { echo "cannot check out midnight-reserve-contracts at $RESERVE_CONTRACTS_PATH" >&2; exit 3; }
git -C "$OLD_ROOT" checkout -q -- local-environment 2>/dev/null || true

rm -rf "$STATE_DIR" "$EVIDENCE_DIR" "$DAPPS_DIR"
init_out_dirs

prepare_old_checkout "$OLD_ROOT"
load_compose_env "$OLD_ROOT" "$OLD_NODE_IMAGE" "$OLD_TOOLKIT_IMAGE"
npm ci --no-audit --no-fund --loglevel=error
bring_up_local_env || { echo "local-env did not come up in three attempts" >&2; exit 3; }
# The only Cardano block producer must not starve while the host is proving.
docker update --cpu-shares 4096 cardano-node-1 > /dev/null 2>&1 || true

echo "waiting for the first finalized block"
for attempt in $(seq 1 60); do
    finalized=$(finalized_height)
    [ -n "$finalized" ] && [ "$finalized" -ge 1 ] && break
    [ "$attempt" = 60 ] && { echo "no finalized block after 5 minutes" >&2; exit 3; }
    sleep 5
done
remember LOCAL_ENV_CHECKOUT "$OLD_ROOT"
echo "local-env is up: node $(node_version), spec $(spec_version), finalized #$(finalized_height)"
