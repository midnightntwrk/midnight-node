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
# cash_out. local-env funds one wallet per actor at genesis (GENESIS_SEEDS).
#
#   IMAGE_TAG    node/toolkit images; must match this checkout's runtime metadata. Default:
#                the CI tag of the nearest ancestor commit with a published image, else latest-main
#   OUTDIR       artifacts (default artifacts/microdao-e2e)
#   REUSE_ENV=1  skip the bring-up and run against the env already up
#   NODE_WS/NODE_HTTP  node RPC endpoints (default local-env node-1)
#   PROOF_SERVER       forwarded to `send-intent --proof-server` (a URL, or `mock` against a
#                      node built with `--features mock-proofs`)
#   SLOTS_PER_EPOCH    sidechain slots per epoch (default 10)
#   CARDANO_EPOCH_LENGTH  local Cardano epoch in seconds (default 60)
#
# The local-env chain cannot reorg, so the toolkit syncs to the best block and `send` returns on
# best-block inclusion (MN_FETCH_BEST / MN_NO_WAIT_FINALITY).
#
# The 14 contract transactions are pipelined: each intent is generated against the contract
# state predicted by the previous call, proving starts once the transactions it depends on are
# in a block, and submission follows program order. The run asserts the final on-chain state
# equals the predicted one. set_topic and buy_in write the pot coin, whose Merkle index the
# ledger assigns on inclusion, so their post-state comes from `send-intent
# --output-onchain-state` against the best block, one block behind the previous transaction;
# every other call is predicted by `generate-intent --output-onchain-state` alone.

set -euo pipefail

# shellcheck source=scripts/tests/microdao/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/microdao/lib.sh"

LOCAL_ENV_DIR=$REPO_ROOT/local-environment
TOOLKIT_JS_DIR=$REPO_ROOT/util/toolkit-js
TEMPLATE=$REPO_ROOT/scripts/tests/microdao/contract.config.template.ts
SRC_CONTRACT=$REPO_ROOT/compact/test-center/test-contracts/micro-dao.compact

default_image_tag() {
    local version arch tree tag
    version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO_ROOT/node/Cargo.toml" | head -1)
    arch=$(uname -m); [[ $arch == x86_64 ]] && arch=amd64; [[ $arch == aarch64 ]] && arch=arm64
    for rev in HEAD~{0..15}; do
        tree=$(git -C "$REPO_ROOT" rev-parse --verify --quiet "$rev^{tree}" | cut -c1-12) || break
        tag=$version-$tree-$arch
        if docker manifest inspect "ghcr.io/midnight-ntwrk/midnight-node:$tag" > /dev/null 2>&1; then
            echo "$tag"; return
        fi
    done
    echo latest-main
}
IMAGE_TAG=${IMAGE_TAG:-$(default_image_tag)}
OUTDIR=${OUTDIR:-$REPO_ROOT/artifacts/microdao-e2e}
NETWORK=${NETWORK:-local}
NODE_WS=${NODE_WS:-ws://127.0.0.1:9933}
NODE_HTTP=${NODE_HTTP:-http://127.0.0.1:9933}
PROOF_SERVER_ARGS=()
[[ -n ${PROOF_SERVER:-} ]] && PROOF_SERVER_ARGS=(--proof-server "$PROOF_SERVER")

ORGANIZER_SECRET_KEY=deadbeefcafebabe1234567890abcdef1122334455667788aabbccddeeff0011
declare -A VOTER_SECRET_KEY=(
    [red]=1111111111111111111111111111111111111111111111111111111111111111
    [blue]=2222222222222222222222222222222222222222222222222222222222222222
    [green]=3333333333333333333333333333333333333333333333333333333333333333
)
declare -A VOTE=([red]=true [blue]=false [green]=true)
VOTERS=(red blue green)
declare -A WRITES_COIN=([set_topic]=1 [buy_in]=1)
NATIVE_TOKEN=0000000000000000000000000000000000000000000000000000000000000000
SEED_DUST=1000000
BUY_IN_DUST=1000000

CONTRACT_DIR=$OUTDIR/contract
COMPILED_CONTRACT=$CONTRACT_DIR/out
CONFIG_FILE=$CONTRACT_DIR/contract.config.ts
COMPILE_CACHE_ROOT=${COMPILE_CACHE_ROOT:-$REPO_ROOT/artifacts/.compactc-cache}

export COMPACTC_VERSION
COMPACTC_VERSION=$(cat "$REPO_ROOT/COMPACTC_VERSION")
export COMPACT_REPO=${COMPACT_REPO:-LFDT-Minokawa/compact}
export TOOLKIT_JS_PATH=$TOOLKIT_JS_DIR
export MIDNIGHT_NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:$IMAGE_TAG
export TOOLKIT_IMAGE=ghcr.io/midnight-ntwrk/midnight-node-toolkit:$IMAGE_TAG
# Session-rotation blocks carry no user transactions; 10 is the largest value a 60s Cardano
# epoch allows, and halves their share compared to local-env's default of 5.
export SLOTS_PER_EPOCH=${SLOTS_PER_EPOCH:-10}
export CARDANO_EPOCH_LENGTH=${CARDANO_EPOCH_LENGTH:-60}
if (( SLOTS_PER_EPOCH < 1 || CARDANO_EPOCH_LENGTH < 1 || (CARDANO_EPOCH_LENGTH * 1000) % (SLOTS_PER_EPOCH * 6000) != 0 )); then
    echo "CARDANO_EPOCH_LENGTH=${CARDANO_EPOCH_LENGTH}s is not an exact multiple of SLOTS_PER_EPOCH=$SLOTS_PER_EPOCH * 6s;" >&2
    echo "the node would fail with 'incoherent consensus timing configuration'." >&2
    exit 1
fi
export MN_FETCH_BEST=true
export MN_NO_WAIT_FINALITY=true

STEP=0

toolkit() {
    "$TOOLKIT_BIN" "$@"
}

# The fetch cache and ledger-state db are single-writer, and actors run concurrently.
actor_toolkit() {
    local actor=$1; shift
    mkdir -p "$OUTDIR/cache/$actor"
    MN_LEDGER_CACHE_DB=$OUTDIR/cache/$actor/ledger_cache_db \
        "$TOOLKIT_BIN" "$@" --fetch-cache "redb:$OUTDIR/cache/$actor/fetch_cache.redb"
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
    actor_toolkit "$1" show-wallet --seed "${SEED[$1]}" -s "$NODE_WS" \
        | jq -c '[.coins[] | {token_type, value}] | group_by(.token_type) | map({key: .[0].token_type, value: (map(.value) | add)}) | from_entries'
}

build_toolkit_in_env() {
    load_direnv "$REPO_ROOT"
    build_toolkit
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

    # compactc output is a pure function of (source, compiler version): reuse it across runs
    local cache_key cached
    cache_key=$(cat "$SRC_CONTRACT" "$REPO_ROOT/COMPACTC_VERSION" | shasum -a 256 | cut -c1-16)
    cached=$COMPILE_CACHE_ROOT/micro-dao-$cache_key
    if [[ -d $cached/out ]]; then
        echo "  compactc cache hit ($cache_key)"
        cp -R "$cached/out" "$COMPILED_CONTRACT"
        return
    fi

    cd "$TOOLKIT_JS_DIR"
    npx fetch-compactc --version="$COMPACTC_VERSION"
    npx run-compactc "$CONTRACT_DIR/micro-dao.compact" "$COMPILED_CONTRACT"

    # compact-js parses CLI args for inline struct types only, not type aliases
    local dts=$COMPILED_CONTRACT/contract/index.d.ts
    sed -e 's/costs_param_0: Costs)/costs_param_0: { seed_dust: bigint; buy_in_dust: bigint })/' \
        -e 's/\(_0: \)ShieldedCoinInfo\([,)]\)/\1{ nonce: Uint8Array; color: Uint8Array; value: bigint }\2/g' \
        "$dts" > "$dts.tmp" && mv "$dts.tmp" "$dts"

    mkdir -p "$cached"
    cp -R "$COMPILED_CONTRACT" "$cached/out"
}

write_contract_config() {
    sed -e "s|{{SECRET_KEY}}|$ORGANIZER_SECRET_KEY|" \
        -e "s|{{COIN_PUBLIC}}|${COIN_PUBLIC[organizer]}|" \
        -e "s|{{NETWORK}}|$NETWORK|" \
        "$TEMPLATE" > "$CONFIG_FILE"
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
    GENESIS_SEEDS=$(genesis_seeds_json) npm run run:local-env
}

wait_for_first_block() {
    "$LOCAL_ENV_DIR/check-health.sh" -u "$NODE_HTTP" -b 1 -t 900
}

wait_for_dust() {
    local actor total
    for actor in "${ACTORS[@]}"; do
        for _ in $(seq 120); do
            total=$(actor_toolkit "$actor" dust-balance --seed "${SEED[$actor]}" -s "$NODE_WS" 2>/dev/null | jq -r '.total // 0' || true)
            [[ -n $total && $total != 0 ]] && break
            sleep 2
        done
        [[ -n $total && $total != 0 ]] || { echo "  timeout waiting for DUST ($actor)"; return 1; }
    done
}

deploy_dao() {
    local prefix=$OUTDIR/0_deploy_organizer
    toolkit generate-intent deploy \
        -c "$CONFIG_FILE" \
        --network "$NETWORK" \
        --coin-public "${COIN_PUBLIC[organizer]}" \
        --output-intent "$prefix.intent.bin" \
        --output-private-state "$OUTDIR/organizer.private_state.json" \
        --output-zswap-state "$prefix.zswap.json" \
        "$ORGANIZER_SECRET_KEY" \
        "{\"seed_dust\": $SEED_DUST, \"buy_in_dust\": $BUY_IN_DUST}"

    local t0
    t0=$(date +%s)
    actor_toolkit organizer send-intent \
        --intent-file "$prefix.intent.bin" \
        --compiled-contract-dir "$COMPILED_CONTRACT" \
        --funding-seed "${SEED[organizer]}" \
        --dest-file "$prefix.tx.mn" \
        "${PROOF_SERVER_ARGS[@]}" \
        -s "$NODE_WS"
    local t1
    t1=$(date +%s)
    toolkit generate-txs --src-file "$prefix.tx.mn" -r 1 send -d "$NODE_WS"
    touch "$prefix.proving" "$prefix.built" "$prefix.included"
    echo "prove=$((t1 - t0)) submitted=$t1 included=$(date +%s)" > "$prefix.timing"
    STEP_PREFIX[0]=$prefix
    ACTOR_LAST_STEP[organizer]=0
    PRED_STATE_PENDING=""

    DAO_ADDR=$(toolkit contract-address --src-file "$prefix.tx.mn")
    for voter in "${VOTERS[@]}"; do
        echo "{\"secretKey\":\"${VOTER_SECRET_KEY[$voter]}\",\"state\":0,\"vote\":null}" > "$OUTDIR/$voter.private_state.json"
    done
    # deploy predicts no state, so the predictions start from the deployed state
    fetch_state "$prefix.state_after.mn"
    PRED_STATE=$prefix.state_after.mn
    echo "  micro-dao address: $DAO_ADDR"
}

rpc() {
    local method=$1 params=${2:-[]}
    curl -sf -H 'content-type: application/json' \
        -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params}" \
        "$NODE_HTTP" | jq -re '.result'
}

LAST_BLOCK=""
LAST_BLOCK_AT=0

# A stalled local Cardano node stops Midnight block production for good; fail fast instead of
# letting every later toolkit call run into its own timeout.
assert_chain_live() {
    local now block
    now=$(date +%s)
    block=$(rpc chain_getHeader | jq -re '.number') || return 0
    if [[ $block != "$LAST_BLOCK" ]]; then
        LAST_BLOCK=$block
        LAST_BLOCK_AT=$now
        return 0
    fi
    (( now - LAST_BLOCK_AT < 60 )) && return 0
    for _ in $(seq 6); do
        sleep 5
        block=$(rpc chain_getHeader | jq -re '.number') || true
        if [[ $block != "$LAST_BLOCK" ]]; then
            LAST_BLOCK=$block
            LAST_BLOCK_AT=$(date +%s)
            return 0
        fi
    done
    echo "" >&2
    echo "chain stopped advancing at block $((LAST_BLOCK)) and has not moved for 90s." >&2
    echo "The Cardano side has most likely stalled; check" >&2
    echo "    docker exec cardano-node-1 tail -5 /data/node.log" >&2
    echo "for 'Could not obtain ledger view'. That state is unrecoverable: rebuild the env." >&2
    echo "Heavy CPU load on the host (a parallel cargo build) is the usual trigger." >&2
    return 1
}

# The bytes `toolkit contract-state` writes, without replaying the chain. Pinned to one block
# hash and rejected if that block stopped being canonical meanwhile.
fetch_state() {
    local out=$1 anchor number hex canonical
    anchor=$(rpc chain_getBlockHash)
    number=$(rpc chain_getHeader "[\"$anchor\"]" | jq -re '.number')
    hex=$(rpc midnight_contractState "[\"$DAO_ADDR\",\"$anchor\"]")
    canonical=$(rpc chain_getBlockHash "[\"$number\"]")
    if [[ $canonical != "$anchor" ]]; then
        echo "contract state read at $anchor (#$((number))) is no longer canonical: $canonical" >&2
        return 1
    fi
    xxd -r -p <<< "$hex" > "$out"
}

# Waits for a step marker; gives up as soon as any step has failed.
wait_marker() {
    while [[ ! -e $1 ]]; do
        if compgen -G "$OUTDIR/*.failed" > /dev/null; then return 1; fi
        sleep 0.2
    done
}

# Concurrent proofs compete for the same cores and finish no sooner, so prove one at a time.
acquire_prove_lock() {
    until mkdir "$OUTDIR/.prove-lock" 2> /dev/null; do sleep 0.2; done
}

release_prove_lock() {
    rmdir "$OUTDIR/.prove-lock"
}

# Background half of a circuit call. Proving waits for the actor's previous tx to be in a block
# (its DUST change is then spendable) and, for a pot writer, for the preceding tx too. Proofs
# start in program order and each step submits once the preceding tx has landed.
# Explicit `|| return 1` throughout: errexit is off inside the `||` that reports failure.
step_job_run() {
    local actor=$1 prefix=$2 same_actor_prev=$3 prev=$4 writes_coin=$5
    local t0 t1 t2
    local predict=()
    [[ -z $same_actor_prev ]] || wait_marker "$same_actor_prev.included" || return 1
    if [[ -n $writes_coin ]]; then
        wait_marker "$prev.included" || return 1
        predict=(--output-onchain-state "$prefix.state_after.mn")
    fi
    wait_marker "$prev.proving" || return 1
    acquire_prove_lock
    touch "$prefix.proving"
    t0=$(date +%s)
    actor_toolkit "$actor" send-intent \
        --intent-file "$prefix.intent.bin" \
        --compiled-contract-dir "$COMPILED_CONTRACT" \
        --zswap-state-file "$prefix.zswap.json" \
        --shielded-destination "${SHIELDED_ADDR[$actor]}" \
        --funding-seed "${SEED[$actor]}" \
        --dest-file "$prefix.tx.mn" \
        "${predict[@]}" \
        "${PROOF_SERVER_ARGS[@]}" \
        -s "$NODE_WS" || { release_prove_lock; return 1; }
    t1=$(date +%s)
    release_prove_lock
    touch "$prefix.built"
    wait_marker "$prev.included" || return 1
    t2=$(date +%s)
    toolkit generate-txs --src-file "$prefix.tx.mn" -r 1 send -d "$NODE_WS" || return 1
    echo "prove=$((t1 - t0)) submitted=$t2 included=$(date +%s)" > "$prefix.timing"
    touch "$prefix.included"
}

step_job() {
    local step=$1 prefix=$3
    step_job_run "$2" "$3" "$4" "$5" "$6" > "$prefix.log" 2>&1 || {
        touch "$prefix.failed"
        echo "step $step ($(basename "$prefix")) failed; see $prefix.log" >&2
        tail -20 "$prefix.log" >&2
        return 1
    }
}

declare -A ACTOR_LAST_STEP
STEP_PREFIX=()
JOB_PIDS=()
PRED_STATE=""
PRED_STATE_PENDING=""

# Foreground half: generate the intent against the predicted state, then hand proving and
# submission to a background job. After a pot writer, the predicted state exists only once its
# job has built the transaction.
call_circuit() {
    local actor=$1 circuit=$2; shift 2
    STEP=$((STEP + 1))
    local prefix=$OUTDIR/${STEP}_${circuit}_$actor
    local private_state=$OUTDIR/$actor.private_state.json
    local writes_coin=${WRITES_COIN[$circuit]:-}

    if compgen -G "$OUTDIR/*.failed" > /dev/null; then
        echo "an earlier step failed" >&2
        return 1
    fi
    assert_chain_live
    echo "▶ $STEP $circuit ($actor)"

    if [[ -n $PRED_STATE_PENDING ]]; then
        wait_marker "$PRED_STATE_PENDING.built" || return 1
        PRED_STATE=$PRED_STATE_PENDING.state_after.mn
        PRED_STATE_PENDING=""
    fi
    local predict=()
    [[ -n $writes_coin ]] || predict=(--output-onchain-state "$prefix.state_after.mn")

    toolkit generate-intent circuit \
        -c "$CONFIG_FILE" \
        --network "$NETWORK" \
        --coin-public "${COIN_PUBLIC[$actor]}" \
        --contract-address "$DAO_ADDR" \
        --input-onchain-state "$PRED_STATE" \
        "${predict[@]}" \
        --input-private-state "$private_state" \
        --output-intent "$prefix.intent.bin" \
        --output-private-state "$prefix.private_state.json" \
        --output-zswap-state "$prefix.zswap.json" \
        --output-result "$prefix.result.json" \
        "${LEDGER_PARAMS_ARGS[@]}" \
        -s "$NODE_WS" \
        "$circuit" "$@"
    cp "$prefix.private_state.json" "$private_state"
    LAST_RESULT=$prefix.result.json
    echo "  result: $(cat "$LAST_RESULT")"

    local same_actor_prev=""
    [[ -z ${ACTOR_LAST_STEP[$actor]:-} ]] || same_actor_prev=${STEP_PREFIX[${ACTOR_LAST_STEP[$actor]}]}
    step_job "$STEP" "$actor" "$prefix" "$same_actor_prev" "${STEP_PREFIX[$((STEP - 1))]}" "$writes_coin" &
    JOB_PIDS+=($!)
    STEP_PREFIX[STEP]=$prefix
    ACTOR_LAST_STEP[$actor]=$STEP
    if [[ -n $writes_coin ]]; then
        PRED_STATE_PENDING=$prefix
    else
        PRED_STATE=$prefix.state_after.mn
    fi
}

wait_for_jobs() {
    local pid failed=0
    for pid in "${JOB_PIDS[@]}"; do
        wait "$pid" || failed=1
    done
    (( failed == 0 )) || { echo "contract flow failed" >&2; exit 1; }
}

contract_flow() {
    CONTRACT_START=$(date +%s)
    deploy_dao
    call_circuit organizer set_topic "test topic" "{\"bytes\":\"${COIN_PUBLIC[organizer]}\"}" "$(fresh_coin $SEED_DUST)"
    for voter in "${VOTERS[@]}"; do
        call_circuit "$voter" buy_in "$(fresh_coin $BUY_IN_DUST)" 1
        VOTING_COIN[$voter]=$(coin_from_result "$LAST_RESULT")
    done
    for voter in "${VOTERS[@]}"; do
        call_circuit "$voter" vote_commit "${VOTE[$voter]}" "${VOTING_COIN[$voter]}"
    done
    call_circuit organizer advance
    for voter in "${VOTERS[@]}"; do
        call_circuit "$voter" vote_reveal
    done
    call_circuit organizer advance
    call_circuit organizer cash_out
    echo ""
    echo "▶ all 14 intents generated in $(( $(date +%s) - CONTRACT_START ))s; waiting for proving and inclusion"
    wait_for_jobs
}

print_step_table() {
    local i prefix name prove submitted included
    printf '%-28s %6s %10s %10s\n' "step" "prove" "submitted" "included"
    for i in $(seq 0 "$STEP"); do
        prefix=${STEP_PREFIX[$i]}
        name=$(basename "$prefix")
        # shellcheck disable=SC2046
        eval "$(cat "$prefix.timing")"
        printf '%-28s %5ss %9ss %9ss\n' "$name" "$prove" "+$((submitted - CONTRACT_START))" "+$((included - CONTRACT_START))"
    done
}

verify_prediction() {
    if cmp -s "$OUTDIR/state_final.mn" "$PRED_STATE"; then
        echo "  PASS: final on-chain state equals the predicted state"
    else
        echo "  FAIL: final on-chain state differs from the predicted state ($PRED_STATE)"
        exit 1
    fi
}

verify_privacy() {
    for f in "$OUTDIR"/*.tx.mn "$OUTDIR/state_final.mn"; do
        assert_secrets_absent "$f"
    done
}

# cash_out pays the pot (seed + 3 buy-ins) to the organizer; each voter paid one buy-in.
verify_funds() {
    local actor before after delta ok=1
    for actor in "${ACTORS[@]}"; do
        before=${FUNDS_BEFORE[$actor]}
        after=$(wallet_funds "$actor")
        if [[ $actor == organizer ]]; then delta=$(( 3 * BUY_IN_DUST )); else delta=$(( -BUY_IN_DUST )); fi
        echo "  $actor before: $before"
        echo "  $actor after:  $after (expected native $delta)"
        jq -en --argjson b "$before" --argjson a "$after" --arg t "$NATIVE_TOKEN" --argjson d "$delta" \
            '($b | .[$t] += $d) == $a' > /dev/null || { echo "  FAIL: $actor funds off"; ok=0; }
    done
    (( ok == 1 )) || exit 1
    echo "  PASS: pot paid to the organizer, voting tokens burned"
}

echo "=== micro-dao e2e on local-env ($IMAGE_TAG, toolkit @ $(git -C "$REPO_ROOT" rev-parse --short HEAD)) ==="
ensure_node
ensure_submodules
rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"

timed "Build toolkit"                          build_toolkit_in_env
timed "Build toolkit-js"                       build_toolkit_js
timed "Compile micro-dao (compactc $COMPACTC_VERSION)" compile_contract
if [[ -z ${REUSE_ENV:-} ]]; then
    timed "Export pinned reserve contracts"    export_reserve_contracts
    timed "Stop previous local-env"            stop_local_env
    timed "Pull $IMAGE_TAG images"             pull_images
    timed "Start local-env (compose up)"       start_local_env
    timed "Wait for first block"               wait_for_first_block
fi
timed "Wait for DUST balance"                  wait_for_dust

declare -A COIN_PUBLIC SHIELDED_ADDR FUNDS_BEFORE
for actor in "${ACTORS[@]}"; do
    COIN_PUBLIC[$actor]=$(toolkit show-address --network "$NETWORK" --seed "${SEED[$actor]}" --coin-public)
    SHIELDED_ADDR[$actor]=$(toolkit show-address --network "$NETWORK" --seed "${SEED[$actor]}" --shielded)
    FUNDS_BEFORE[$actor]=$(wallet_funds "$actor")
done
# Read once; every generate-intent would otherwise ask the node.
LEDGER_PARAMS_ARGS=()
if params=$(toolkit show-ledger-parameters --serialize --read-from-rpc-url "$NODE_WS" 2>/dev/null | tail -1) && [[ -n $params ]]; then
    LEDGER_PARAMS_ARGS=(--custom-ledger-parameters "$params")
fi
write_contract_config

declare -A VOTING_COIN
timed "Contract flow (14 txs, pipelined)"      contract_flow
timed "Fetch final contract state"             fetch_state "$OUTDIR/state_final.mn"
timed "Verify predicted state"                 verify_prediction
timed "Verify secret keys never on-chain"      verify_privacy
timed "Verify wallet funds"                    verify_funds

print_timings
print_step_table
echo "============================================================"
echo "  contract: $DAO_ADDR"
echo "  outputs:  $OUTDIR"
