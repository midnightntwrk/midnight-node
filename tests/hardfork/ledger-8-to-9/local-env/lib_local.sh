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

# local-environment plumbing: the release worktrees, their compose environment, bring-up.
TARGET=local
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

local_env_dir() { echo "$1/local-environment"; }   # <checkout>
compose_dir()   { echo "$1/local-environment/src/networks/local-env"; }

# midnight-reserve-contracts at the commit the ledger-9 ref pins; node-1.0.x has no submodule.
RESERVE_CONTRACTS_PATH="${RESERVE_CONTRACTS_PATH:-$HF_WORK_DIR/midnight-reserve-contracts}"
reserve_contracts_ready() {
    local commit
    commit=$(git -C "$REPO_ROOT" ls-tree "${L9_REF:-HEAD}" midnight-reserve-contracts 2>/dev/null | awk '{print $3}')
    [ -n "$commit" ] || commit=$(git -C "$REPO_ROOT" ls-tree HEAD midnight-reserve-contracts | awk '{print $3}')
    [ -d "$RESERVE_CONTRACTS_PATH/.git" ] || git clone -q https://github.com/midnightntwrk/midnight-reserve-contracts "$RESERVE_CONTRACTS_PATH" || return 1
    [ "$(git -C "$RESERVE_CONTRACTS_PATH" rev-parse HEAD)" = "$commit" ] && return 0
    git -C "$RESERVE_CONTRACTS_PATH" fetch -q origin "$commit" 2>/dev/null || git -C "$RESERVE_CONTRACTS_PATH" fetch -q origin
    git -C "$RESERVE_CONTRACTS_PATH" checkout -q --detach "$commit"
}

# .envrc derives tree-hash image tags, unpublished for release tags: the release images
# override them.
load_compose_env() {  # <checkout> <node-image> <toolkit-image>
    local le; le=$(local_env_dir "$1")
    cd "$le" || return 1
    export INDEXER_TAG
    # shellcheck source=/dev/null
    source .envrc > /dev/null 2>&1
    if [ -s .env.default ]; then set -a; source .env.default; set +a; fi
    local f
    for f in localenv_postgres.password:LOCALENV_POSTGRES_PASSWORD localenv_app_storage.password:APP__INFRA__STORAGE__PASSWORD \
             localenv_pubsub.password:APP__INFRA__PUB_SUB__PASSWORD localenv_app_infra_secret.password:APP__INFRA__SECRET; do
        [ -s "${f%%:*}" ] && export "${f#*:}=$(cat "${f%%:*}")"
    done
    export MIDNIGHT_NODE_IMAGE="$2" TOOLKIT_IMAGE="$3"
    MIDNIGHT_NODE_TAG="${2##*:}"; export MIDNIGHT_NODE_TAG="${MIDNIGHT_NODE_TAG%-"$ARCH"}"
    export MIDNIGHT_RESERVE_CONTRACTS_PATH="$RESERVE_CONTRACTS_PATH"
    export INDEXER_CHAIN_IMAGE="$IMAGE_REGISTRY/chain-indexer:$INDEXER_TAG"
    export INDEXER_WALLET_IMAGE="$IMAGE_REGISTRY/wallet-indexer:$INDEXER_TAG"
    export INDEXER_API_IMAGE="$IMAGE_REGISTRY/indexer-api:$INDEXER_TAG"
}

# node-1.0.x pins Debian package versions that are gone from the mirrors.
unpin_contract_compiler_apt() {  # <checkout>
    local f; f="$(compose_dir "$1")/configurations/contract-compiler/Dockerfile"
    [ -f "$f" ] && sed_i -E 's/^([[:space:]]+)([a-z][a-z0-9.+-]+)=[^[:space:]]+/\1\2/' "$f"
    return 0
}

# Started before systemStart, the cardano node backs off 60 s and, with a 60-slot epoch,
# never forges.
patch_cardano_start() {  # <checkout>
    python3 - "$(compose_dir "$1")/configurations/cardano/entrypoint.sh" <<'PY'
import sys
p = sys.argv[1]; s = open(p).read()
marker = "start the node only after systemStart"
if marker in s:
    sys.exit(0)
old = '\nstart_node\n\n# Wait for genesis time to arrive and node to be ready before submitting transactions\n'
if s.count(old) != 1:
    sys.exit("cardano entrypoint anchor not found")
new = ('\n# ' + marker + '\nwhile [ "$(date +%s)" -le "$target_time" ]; do sleep 1; done\n' + old)
open(p, "w").write(s.replace(old, new))
PY
}

ogmios_tip_slot() {
    curl -s -m 5 -X POST "http://localhost:${OGMIOS_PORT:-1337}" -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","method":"queryNetwork/tip"}' | jq -r '.result.slot // empty' 2>/dev/null
}
cardano_chain_alive() {
    local s1 s2 i; s1=$(ogmios_tip_slot)
    for i in $(seq 1 10); do sleep 2; s2=$(ogmios_tip_slot); [ -n "$s2" ] && [ "$s2" -gt "${s1:-0}" ] && return 0; done
    return 1
}

# Also the services this checkout's compose file does not define: they would pin old volumes.
teardown_local_env() {  # cwd: a local-environment directory
    local ids vols
    ids=$(docker ps -aq --filter label=com.docker.compose.project=local-env)
    [ -n "$ids" ] && docker rm -f $ids >/dev/null
    npm run --silent stop:local-env-with-indexer >/dev/null 2>&1 || true
    vols=$(docker volume ls -q | grep '^local-env_' || true)
    [ -z "$vols" ] || docker volume rm $vols >/dev/null
}
bring_up_local_env() {  # cwd: a local-environment directory
    local attempt
    for attempt in 1 2 3; do
        echo "--- bring-up attempt $attempt/3 ---"
        teardown_local_env
        if npm run run:local-env-with-indexer; then cardano_chain_alive && return 0; fi
        # A one-shot service can lose a race against the genesis funding tx on a live chain.
        if cardano_chain_alive; then sleep 20; npm run run:local-env-with-indexer && cardano_chain_alive && return 0; fi
    done
    return 1
}

# The chain started from the ledger-8 checkout, whose network id is "undeployed"; every
# transaction carries it.
indexer_compose() {  # <checkout> <compose args...>
    local ck=$1 ov; shift
    ov="$CACHE_DIR/indexer-network-override.yml"
    cat > "$ov" <<'YML'
services:
  chain-indexer:
    environment:
      APP__APPLICATION__NETWORK_ID: "undeployed"
  wallet-indexer:
    environment:
      APP__APPLICATION__NETWORK_ID: "undeployed"
  indexer-api:
    environment:
      APP__APPLICATION__NETWORK_ID: "undeployed"
YML
    ( cd "$(compose_dir "$ck")" && docker compose -f docker-compose.yml -f "$ov" --profile withindexer "$@" )
}
