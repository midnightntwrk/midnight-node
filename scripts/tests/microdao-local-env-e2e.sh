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

# Timed micro-dao e2e on a fresh local-env (mirrors midnight-ledger/ledger/tests/micro-dao.rs):
# deploy, set_topic, buy_in + vote_commit per voter, advance, vote_reveal per voter, advance,
# cash_out. The local genesis is regenerated with a shielded-funded DAO wallet for the run.
#
#   IMAGE_TAG    node/toolkit images (default latest-main; must match this checkout)
#   OUTDIR       artifacts (default artifacts/microdao-e2e)
#   REUSE_ENV=1  skip the bring-up, run the contract phases against the env already up

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
LOCAL_ENV_DIR=$REPO_ROOT/local-environment
TOOLKIT_JS_DIR=$REPO_ROOT/util/toolkit-js
TOOLKIT_BIN=$REPO_ROOT/target/release/midnight-node-toolkit
GENESIS_DIR=$REPO_ROOT/res/genesis
TEMPLATE=$REPO_ROOT/scripts/tests/microdao/contract.config.template.ts
SRC_CONTRACT=$REPO_ROOT/compact/test-center/test-contracts/micro-dao.compact

IMAGE_TAG=${IMAGE_TAG:-latest-main}
OUTDIR=${OUTDIR:-$REPO_ROOT/artifacts/microdao-e2e}
NETWORK=local
NODE_WS=ws://127.0.0.1:9933
NODE_HTTP=http://127.0.0.1:9933

SEED=0000000000000000000000000000000000000000000000000000000000000002
ORGANIZER_SECRET_KEY=deadbeefcafebabe1234567890abcdef1122334455667788aabbccddeeff0011
declare -A VOTER_SECRET_KEY=(
    [red]=1111111111111111111111111111111111111111111111111111111111111111
    [blue]=2222222222222222222222222222222222222222222222222222222222222222
    [green]=3333333333333333333333333333333333333333333333333333333333333333
)
declare -A VOTE=([red]=true [blue]=false [green]=true)
VOTERS=(red blue green)
NATIVE_TOKEN=0000000000000000000000000000000000000000000000000000000000000000
SEED_DUST=1000000
BUY_IN_DUST=1000000

CONTRACT_DIR=$OUTDIR/contract
COMPILED_CONTRACT=$CONTRACT_DIR/out
CONFIG_FILE=$CONTRACT_DIR/contract.config.ts
FETCH_CACHE=redb:$OUTDIR/fetch_cache.redb

export COMPACTC_VERSION
COMPACTC_VERSION=$(cat "$REPO_ROOT/COMPACTC_VERSION")
export COMPACT_REPO=${COMPACT_REPO:-LFDT-Minokawa/compact}
export TOOLKIT_JS_PATH=$TOOLKIT_JS_DIR
export MIDNIGHT_NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:$IMAGE_TAG
export TOOLKIT_IMAGE=ghcr.io/midnight-ntwrk/midnight-node-toolkit:$IMAGE_TAG

TIMINGS=()
SCRIPT_START=$(date +%s)
STEP=0

timed() {
    local label=$1; shift
    echo ""
    echo "▶ $label"
    local start
    start=$(date +%s)
    "$@"
    local secs=$(( $(date +%s) - start ))
    TIMINGS+=("$(printf '%-48s %6ds' "$label" "$secs")")
    echo "  ✓ ${secs}s"
}

toolkit() {
    "$TOOLKIT_BIN" "$@"
}

load_direnv() {
    cd "$1"
    direnv allow
    eval "$(direnv export bash)"
    export PATH=$NODE_BIN_DIR:$PATH   # direnv restores the original PATH, undoing `nvm use`
}

load_local_env() {
    load_direnv "$LOCAL_ENV_DIR"
    export MIDNIGHT_RESERVE_CONTRACTS_PATH=$OUTDIR/reserve-contracts   # always the pinned submodule commit
}

ensure_node() {
    local major
    major=$(grep -m1 'NODEJS_VERSION=' "$REPO_ROOT/Earthfile" | sed 's/.*=\([0-9]*\).*/\1/')
    if [[ "$(node --version 2>/dev/null)" != v$major.* && -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]]; then
        # shellcheck disable=SC1091
        . "${NVM_DIR:-$HOME/.nvm}/nvm.sh" && nvm use "$major" > /dev/null
    fi
    [[ "$(node --version)" == v$major.* ]] || { echo "Node $major required"; exit 1; }
    NODE_BIN_DIR=$(dirname "$(command -v node)")
}

ensure_submodules() {
    [[ -f $SRC_CONTRACT ]] || { echo "run: git submodule update --init compact"; exit 1; }
    RESERVE_CONTRACTS_REV=$(git -C "$REPO_ROOT" ls-tree HEAD midnight-reserve-contracts | awk '{print $3}')
    git -C "$REPO_ROOT/midnight-reserve-contracts" cat-file -e "$RESERVE_CONTRACTS_REV" 2>/dev/null \
        || { echo "run: git submodule update --init midnight-reserve-contracts"; exit 1; }
}

fresh_coin() {
    printf '{"nonce":"%s","color":"%s","value":%s}' "$(openssl rand -hex 32)" "$NATIVE_TOKEN" "$1"
}

coin_from_result() {
    python3 -c "
import json
r = json.load(open('$1'))
hex = lambda v: bytes(v).hex()
print(json.dumps({'nonce': hex(r['nonce']), 'color': hex(r['color']), 'value': int(r['value'])}))"
}

assert_secrets_absent() {
    local hex
    hex=$(od -An -v -tx1 "$1" | tr -d ' \n')
    for sk in "$ORGANIZER_SECRET_KEY" "${VOTER_SECRET_KEY[@]}"; do
        if grep -qi "$sk" "$1" || [[ $hex == *$sk* ]]; then
            echo "  FAIL: secret key $sk found in $(basename "$1")"
            exit 1
        fi
    done
    echo "  PASS: no secret key in $(basename "$1")"
}

wallet_funds() {
    toolkit show-wallet --seed "$SEED" -s "$NODE_WS" --fetch-cache "$FETCH_CACHE" \
        | jq -c '[.coins[] | {token_type, value}] | group_by(.token_type) | map({key: .[0].token_type, value: (map(.value) | add)}) | from_entries'
}

build_toolkit() {
    load_direnv "$REPO_ROOT"
    cargo build --release -p midnight-node-toolkit
}

build_toolkit_js() {
    cd "$TOOLKIT_JS_DIR"
    npm ci
    npm run build
}

compile_contract() {
    rm -rf "$CONTRACT_DIR"
    mkdir -p "$CONTRACT_DIR"
    ln -sfn "$TOOLKIT_JS_DIR/node_modules" "$CONTRACT_DIR/node_modules"
    cp "$SRC_CONTRACT" "$CONTRACT_DIR/micro-dao.compact"

    cd "$TOOLKIT_JS_DIR"
    npx fetch-compactc --version="$COMPACTC_VERSION"
    npx run-compactc "$CONTRACT_DIR/micro-dao.compact" "$COMPILED_CONTRACT"

    # compact-js parses CLI args for inline struct types only, not type aliases
    local dts=$COMPILED_CONTRACT/contract/index.d.ts
    sed -e 's/costs_param_0: Costs)/costs_param_0: { seed_dust: bigint; buy_in_dust: bigint })/' \
        -e 's/\(_0: \)ShieldedCoinInfo\([,)]\)/\1{ nonce: Uint8Array; color: Uint8Array; value: bigint }\2/g' \
        "$dts" > "$dts.tmp" && mv "$dts.tmp" "$dts"
}

write_contract_config() {
    sed -e "s|{{SECRET_KEY}}|$ORGANIZER_SECRET_KEY|" \
        -e "s|{{COIN_PUBLIC}}|$COIN_PUBLIC|" \
        -e "s|{{NETWORK}}|$NETWORK|" \
        "$TEMPLATE" > "$CONFIG_FILE"
}

restore_genesis() {
    cp "$OUTDIR"/genesis_backup/*.mn "$GENESIS_DIR/"
}

fund_genesis() {
    echo "{\"dao-wallet\": \"$SEED\"}" > "$OUTDIR/genesis-seeds.json"
    toolkit generate-genesis \
        --network "$NETWORK" \
        --seeds-file "$OUTDIR/genesis-seeds.json" \
        --shielded-num-funding-outputs 2 --shielded-mint-amount 1000000000 \
        --unshielded-num-funding-outputs 1 --unshielded-mint-amount 1000000000000 \
        --ledger-parameters-config "$REPO_ROOT/res/local/ledger-parameters-config.json" \
        --cnight-generates-dust-config "$REPO_ROOT/res/local/cnight-config.json" \
        --ics-config "$REPO_ROOT/res/local/ics-config.json" \
        --reserve-config "$REPO_ROOT/res/local/reserve-config.json" \
        -o "$OUTDIR/genesis"

    mkdir -p "$OUTDIR/genesis_backup"
    cp "$GENESIS_DIR"/genesis_{state,block}_local.mn "$OUTDIR/genesis_backup/"
    trap restore_genesis EXIT
    cp "$OUTDIR"/genesis/genesis_{state,block}_local.mn "$GENESIS_DIR/"
}

export_reserve_contracts() {
    mkdir -p "$OUTDIR/reserve-contracts"
    git -C "$REPO_ROOT/midnight-reserve-contracts" archive "$RESERVE_CONTRACTS_REV" | tar -x -C "$OUTDIR/reserve-contracts"
}

stop_local_env() {
    load_local_env
    npm run stop:local-env
}

pull_images() {
    docker pull "$MIDNIGHT_NODE_IMAGE"
    docker pull "$TOOLKIT_IMAGE"
}

start_local_env() {
    load_local_env
    npm run run:local-env
    restore_genesis
}

wait_for_first_block() {
    "$LOCAL_ENV_DIR/check-health.sh" -u "$NODE_HTTP" -b 1 -t 900
}

wait_for_faucet() {
    local status
    for _ in $(seq 180); do
        status=$(docker inspect -f '{{.State.Status}}:{{.State.ExitCode}}' init-mnight-faucet 2>/dev/null || echo pending)
        case $status in
            exited:0) return 0 ;;
            exited:*) docker logs init-mnight-faucet 2>&1 | tail -20; return 1 ;;
        esac
        sleep 5
    done
    echo "  timeout waiting for init-mnight-faucet"
    return 1
}

wait_for_dust() {
    local total
    for _ in $(seq 120); do
        total=$(toolkit dust-balance --seed "$SEED" -s "$NODE_WS" --fetch-cache "$FETCH_CACHE" 2>/dev/null | jq -r '.total // 0' || true)
        [[ -n $total && $total != 0 ]] && return 0
        sleep 5
    done
    echo "  timeout waiting for DUST"
    return 1
}

deploy_dao() {
    toolkit generate-intent deploy \
        -c "$CONFIG_FILE" \
        --network "$NETWORK" \
        --coin-public "$COIN_PUBLIC" \
        --output-intent "$OUTDIR/0_deploy.intent.bin" \
        --output-private-state "$OUTDIR/organizer.private_state.json" \
        --output-zswap-state "$OUTDIR/0_deploy.zswap.json" \
        "$ORGANIZER_SECRET_KEY" \
        "{\"seed_dust\": $SEED_DUST, \"buy_in_dust\": $BUY_IN_DUST}"

    toolkit send-intent \
        --intent-file "$OUTDIR/0_deploy.intent.bin" \
        --compiled-contract-dir "$COMPILED_CONTRACT" \
        --funding-seed "$SEED" \
        --dest-file "$OUTDIR/0_deploy.tx.mn" \
        --fetch-cache "$FETCH_CACHE" \
        -s "$NODE_WS"

    toolkit generate-txs --src-file "$OUTDIR/0_deploy.tx.mn" -r 1 send -d "$NODE_WS"

    DAO_ADDR=$(toolkit contract-address --src-file "$OUTDIR/0_deploy.tx.mn")
    for voter in "${VOTERS[@]}"; do
        echo "{\"secretKey\":\"${VOTER_SECRET_KEY[$voter]}\",\"state\":0,\"vote\":null}" > "$OUTDIR/$voter.private_state.json"
    done
    echo "  micro-dao address: $DAO_ADDR"
}

fetch_state() {
    toolkit contract-state \
        --contract-address "$DAO_ADDR" \
        --dest-file "$1" \
        --fetch-cache "$FETCH_CACHE" \
        -s "$NODE_WS"
}

call_circuit() {
    local actor=$1 circuit=$2; shift 2
    STEP=$((STEP + 1))
    local prefix=$OUTDIR/${STEP}_${circuit}_$actor
    local private_state=$OUTDIR/$actor.private_state.json

    fetch_state "$prefix.state_before.mn"

    toolkit generate-intent circuit \
        -c "$CONFIG_FILE" \
        --network "$NETWORK" \
        --coin-public "$COIN_PUBLIC" \
        --contract-address "$DAO_ADDR" \
        --input-onchain-state "$prefix.state_before.mn" \
        --input-private-state "$private_state" \
        --output-intent "$prefix.intent.bin" \
        --output-private-state "$prefix.private_state.json" \
        --output-zswap-state "$prefix.zswap.json" \
        --output-result "$prefix.result.json" \
        --fetch-cache "$FETCH_CACHE" \
        -s "$NODE_WS" \
        "$circuit" "$@"

    toolkit send-intent \
        --intent-file "$prefix.intent.bin" \
        --compiled-contract-dir "$COMPILED_CONTRACT" \
        --zswap-state-file "$prefix.zswap.json" \
        --shielded-destination "$SHIELDED_ADDR" \
        --funding-seed "$SEED" \
        --dest-file "$prefix.tx.mn" \
        --fetch-cache "$FETCH_CACHE" \
        -s "$NODE_WS"

    toolkit generate-txs --src-file "$prefix.tx.mn" -r 1 send -d "$NODE_WS"

    cp "$prefix.private_state.json" "$private_state"
    LAST_RESULT=$prefix.result.json
    echo "  result: $(cat "$LAST_RESULT")"
}

verify_privacy() {
    for f in "$OUTDIR"/*.tx.mn "$OUTDIR/state_final.mn"; do
        assert_secrets_absent "$f"
    done
}

verify_funds() {
    FUNDS_AFTER=$(wallet_funds)
    echo "  before: $FUNDS_BEFORE"
    echo "  after:  $FUNDS_AFTER"
    [[ $FUNDS_BEFORE == "$FUNDS_AFTER" ]] || { echo "  FAIL: wallet funds changed"; exit 1; }
    echo "  PASS: pot returned, voting tokens burned"
}

echo "=== micro-dao e2e on local-env ($IMAGE_TAG, toolkit @ $(git -C "$REPO_ROOT" rev-parse --short HEAD)) ==="
ensure_node
ensure_submodules
rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"

timed "Build toolkit (cargo release)"          build_toolkit
timed "Build toolkit-js"                       build_toolkit_js
timed "Compile micro-dao (compactc $COMPACTC_VERSION)" compile_contract
if [[ -z ${REUSE_ENV:-} ]]; then
    timed "Generate funded local genesis"      fund_genesis
    timed "Export pinned reserve contracts"    export_reserve_contracts
    timed "Stop previous local-env"            stop_local_env
    timed "Pull $IMAGE_TAG images"             pull_images
    timed "Start local-env (compose up)"       start_local_env
    timed "Wait for first block"               wait_for_first_block
    timed "Wait for faucet"                    wait_for_faucet
fi
timed "Wait for DUST balance"                  wait_for_dust

COIN_PUBLIC=$(toolkit show-address --network "$NETWORK" --seed "$SEED" --coin-public)
SHIELDED_ADDR=$(toolkit show-address --network "$NETWORK" --seed "$SEED" --shielded)
FUNDS_BEFORE=$(wallet_funds)
write_contract_config

declare -A VOTING_COIN
timed "Deploy micro-dao"                       deploy_dao
timed "set_topic()"                            call_circuit organizer set_topic "test topic" "{\"bytes\":\"$COIN_PUBLIC\"}" "$(fresh_coin $SEED_DUST)"
for voter in "${VOTERS[@]}"; do
    timed "buy_in() $voter"                    call_circuit "$voter" buy_in "$(fresh_coin $BUY_IN_DUST)" 1
    VOTING_COIN[$voter]=$(coin_from_result "$LAST_RESULT")
done
for voter in "${VOTERS[@]}"; do
    timed "vote_commit() $voter -> ${VOTE[$voter]}" call_circuit "$voter" vote_commit "${VOTE[$voter]}" "${VOTING_COIN[$voter]}"
done
timed "advance() commit -> reveal"             call_circuit organizer advance
for voter in "${VOTERS[@]}"; do
    timed "vote_reveal() $voter"               call_circuit "$voter" vote_reveal
done
timed "advance() reveal -> final"              call_circuit organizer advance
timed "cash_out()"                             call_circuit organizer cash_out
timed "Fetch final contract state"             fetch_state "$OUTDIR/state_final.mn"
timed "Verify secret keys never on-chain"      verify_privacy
timed "Verify wallet funds"                    verify_funds

echo ""
echo "============================================================"
printf '%s\n' "${TIMINGS[@]}"
printf '%-48s %6ds\n' "TOTAL" $(( $(date +%s) - SCRIPT_START ))
echo "============================================================"
echo "  contract: $DAO_ADDR"
echo "  outputs:  $OUTDIR"
