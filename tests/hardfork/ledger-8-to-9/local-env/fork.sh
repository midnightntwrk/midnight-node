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

# Table HF: the fork as a network does it, binaries in waves, then the governance runtime
# upgrade. HF_WAVES lists the waves (default "1 2,3,4,5"); MANUAL_UPGRADE=1 stops before
# set_code and prints what to submit in Polkadot.js Apps.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/lib_local.sh"

HF_WAVES="${HF_WAVES:-1 2,3,4,5}"
EV="$EVIDENCE_DIR/hf"; mkdir -p "$EV"
require_baseline
state_load "$PRE_FORK_STATE"
SPEC_BEFORE="${SPEC_BEFORE:-$(spec_version)}"
[ "$(spec_version)" = "$SPEC_BEFORE" ] || cannot_run "the chain is on spec $(spec_version), not $SPEC_BEFORE: forked already?"

t_table HF "Hard fork on local-env: $L8_REF -> ${L9_REF:-${L9_PR:+PR $L9_PR}}, waves '$HF_WAVES'"

c_checkout() {
    local ref="${L9_REF:-}"
    if [ -z "$ref" ] && [ -n "${L9_PR:-}" ]; then
        git -C "$REPO_ROOT" fetch -q origin "pull/$L9_PR/head:refs/remotes/origin/pr-$L9_PR" && ref="origin/pr-$L9_PR"
    fi
    [ -n "$ref" ] || { t_fail "set L9_REF (a tag or commit) or L9_PR"; return; }
    ensure_worktree "$L9_ROOT" "$ref" || { t_fail "cannot create the $ref worktree"; return; }
    git -C "$L9_ROOT" checkout -q -- local-environment 2>/dev/null || true
    N9=$(l9_node_image); T9=$(l9_toolkit_image)
    require_image "$N9" && require_image "$T9" || { t_fail "images $N9 / $T9 missing"; return; }
    # Operators keep their databases in the separate layout across the swap; upstream
    # local-env runs some nodes unified, which fails here.
    local n f
    for n in 1 2 3 4 5; do
        f="$(compose_dir "$L9_ROOT")/configurations/midnight-nodes/midnight-node-$n/entrypoint.sh"
        [ -f "$f" ] && sed_i 's/^export STORAGE_SEPARATION=unified$/export STORAGE_SEPARATION=separate/' "$f"
    done
    # Recreated services must connect with the running stack's credentials.
    cp "$(local_env_dir "$L8_ROOT")"/localenv_*.password "$(local_env_dir "$L9_ROOT")/" || { t_fail "no compose secrets in the ledger-8 checkout"; return; }
    rm -rf "$(compose_dir "$L9_ROOT")/runtime-values"
    cp -R "$(compose_dir "$L8_ROOT")/runtime-values" "$(compose_dir "$L9_ROOT")/" 2>/dev/null || true
    ( load_compose_env "$L9_ROOT" "$N9" "$T9" && npm ci --no-audit --no-fund --loglevel=error ) > "$EV/npm_ci.log" 2>&1 \
        || { t_fail "npm ci in the ledger-9 local-environment (evidence/hf/npm_ci.log)"; return; }
    state_set "$STATE_DIR/env.env" LOCAL_ENV_CHECKOUT "$L9_ROOT"
    t_pass "$(git -C "$L9_ROOT" log -1 --format='%h %s' | cut -c1-80); node $N9; toolkit $T9"
}
t_check HF-SWAP-1 "HF-01" "ledger-9 worktree at ${L9_REF:-PR ${L9_PR:-?}} and its node and toolkit images" c_checkout
t_passed || cannot_run "no usable ledger-9 checkout; the validators are still on $L8_REF"
N9=$(l9_node_image); T9=$(l9_toolkit_image)
compose9() { ( load_compose_env "$L9_ROOT" "$N9" "$T9" >/dev/null && cd "$(compose_dir "$L9_ROOT")" && docker compose "$@" ); }

c_postgres() {
    compose9 up -d --no-deps postgres db-sync > "$EV/compose_postgres.log" 2>&1 || { t_fail "compose up postgres db-sync"; return; }
    local i st h0
    for i in $(seq 1 30); do st=$(docker inspect -f '{{.State.Health.Status}}' postgres 2>/dev/null || echo none); [ "$st" = healthy ] && break; sleep 2; done
    [ "$st" = healthy ] || { t_fail "postgres $st after the recreate"; return; }
    local ssl; ssl=$(docker exec postgres sh -c 'psql -U "${POSTGRES_USER:-postgres}" -d postgres -tAc "show ssl"' 2>/dev/null | tr -d '[:space:]')
    [ "$ssl" = on ] || { t_fail "postgres ssl is '${ssl:-unreadable}' after the recreate"; return; }
    h0=$(node_height); wait_blocks 3 > "$EV/postgres_blocks.txt" || { t_fail "after the recreate: $(cat "$EV/postgres_blocks.txt")"; return; }
    t_pass "postgres healthy with TLS; the ledger-8 fleet still produces ($h0 -> $(node_height))"
}
t_check HF-SWAP-2 "HF-13" "postgres and db-sync recreated for the 2.x node; the old fleet keeps producing" c_postgres

wave=0 moved=0
for w in $HF_WAVES; do
    wave=$((wave + 1)); nodes=$(echo "$w" | tr ',' ' '); moved=$((moved + $(wc -w <<< "$nodes")))
    c_wave() {
        local n names="" v bad=""
        for n in $nodes; do names="$names midnight-node-$n"; done
        compose9 config -q > "$EV/compose_wave$wave.log" 2>&1 || { t_fail "the ledger-9 compose project does not load; nothing stopped"; return; }
        docker stop $names >/dev/null
        compose9 up -d --no-deps $names > "$EV/compose_wave$wave.log" 2>&1 || { t_fail "compose up$names"; return; }
        for n in $nodes; do
            local url; url=$(node_url "midnight-node-$n")
            for i in $(seq 1 24); do v=$(node_version_at "$url"); [ -n "$v" ] && break; sleep 5; done
            [[ "$v" == "$L9_EXPECTED_NODE_PREFIX"* ]] || bad="$bad midnight-node-$n='$v'"
        done
        [ -n "$bad" ] && { t_fail "not on $L9_EXPECTED_NODE_PREFIX:$bad"; return; }
        if WAVE_EXPECTED_L9=$moved "$SUITE_DIR/checks/wave_smoke.sh" "$wave" > "${RUN_DIR:-$RUNS_DIR}/wave$wave.log" 2>&1; then
            t_pass "$names on $L9_EXPECTED_NODE_PREFIX; smoke: $(table_counts "${RUN_DIR:-$RUNS_DIR}/wave$wave.log")"
        else t_fail "smoke failed: $(table_counts "${RUN_DIR:-$RUNS_DIR}/wave$wave.log")"; fi
    }
    t_check "HF-WAVE-$wave" "HF-13" "wave $wave: node(s) $w on the ledger-9 binary, runtime still ledger 8; smoke on the mixed fleet" c_wave
    t_passed || cannot_run "wave $wave failed; the later waves were not started"
done

c_all_nodes() {
    local n v bad="" cnt=0
    for n in "${NODES[@]}"; do cnt=$((cnt + 1)); v=$(node_version_at "${n#*=}"); [[ "$v" == "$L9_EXPECTED_NODE_PREFIX"* ]] || bad="$bad ${n%%=*}='$v'"; done
    [ -z "$bad" ] && t_pass "$cnt/$cnt on $(node_version)" || t_fail "$bad"
}
c_spec_unchanged() { local s; s=$(spec_version); [ "$s" = "$SPEC_BEFORE" ] && t_pass "spec $s: the runtime upgrade is still needed" || t_fail "spec changed to $s before the upgrade"; }
c_indexer_alive() {
    local s restarted=""
    for s in chain-indexer wallet-indexer indexer-api; do
        docker inspect -f '{{.State.Running}}' "$s" 2>/dev/null | grep -q true || { docker start "$s" >/dev/null; restarted="$restarted $s"; }
    done
    sleep 5
    for s in chain-indexer wallet-indexer indexer-api; do docker inspect -f '{{.State.Running}}' "$s" 2>/dev/null | grep -q true || { t_fail "$s not running"; return; }; done
    t_pass "indexer services running${restarted:+ (restarted:$restarted)}"
}
c_agree() {
    local fin; fin=$(finalized_height)
    check_state_agreement "$(block_hash_at "$NODE_HTTP" "$fin")" > "$EV/agreement_after_swap.txt" 2>&1 \
        && t_pass "roots agree at finalized #$fin" || t_fail "$(head -2 "$EV/agreement_after_swap.txt" | tr '\n' ' ')"
}
t_check HF-SWAP-3 "HF-01" "every validator on the ledger-9 binary" c_all_nodes
t_check HF-SWAP-4 "HF-01" "runtime spec unchanged after the binary swap" c_spec_unchanged
t_check HF-SWAP-5 "HF-12" "the indexer survived the swap" c_indexer_alive
t_check HF-SWAP-6 "HF-13" "cross-node agreement after the swap" c_agree

# A release ships the srtool build and its digest, which governance enacts. Other refs
# take the runtime from the CI node image.
WASM_DIR="$(local_env_dir "$L9_ROOT")/artifacts/test"; WASM="$WASM_DIR/midnight_node_runtime.compact.compressed.wasm"
c_wasm() {
    rm -rf "$WASM_DIR/release" "$WASM"; mkdir -p "$WASM_DIR/release"
    local src="" v; v=$(release_version "${L9_REF:-}")
    if [ -n "$v" ] && [ "${RUNTIME_WASM_SOURCE:-release}" = release ]; then
        gh release download "$L9_REF" -R midnightntwrk/midnight-node -p '*.compact.compressed.wasm' -p 'srtool-digest.json' \
            -D "$WASM_DIR/release" --clobber >/dev/null 2>&1 || true
        local rel="$WASM_DIR/release/midnight_node_runtime-$v.compact.compressed.wasm"
        [ -s "$rel" ] && { cp "$rel" "$WASM"; src="release asset $(basename "$rel")"; }
    fi
    if [ -z "$src" ]; then
        docker run --rm --entrypoint cat "$N9" "/artifacts-$ARCH/midnight_node_runtime.compact.compressed.wasm" > "$WASM" 2>/dev/null || true
        src="node image $N9"
    fi
    [ -s "$WASM" ] || { t_fail "no runtime wasm ($src)"; return; }
    WASM_BLAKE2=$(python3 -c "import hashlib,sys; print(hashlib.blake2b(open(sys.argv[1],'rb').read(), digest_size=32).hexdigest())" "$WASM")
    local sha note=""; sha=$(sha256_of "$WASM")
    if [ -s "$WASM_DIR/release/srtool-digest.json" ]; then
        local dsha db2
        dsha=$(jq -r '.. | objects | select(.compressed | type == "object") | .compressed.sha256 // empty' "$WASM_DIR/release/srtool-digest.json" | head -1)
        db2=$(jq -r '.. | objects | select(.compressed | type == "object") | .compressed.blake2_256 // empty' "$WASM_DIR/release/srtool-digest.json" | head -1)
        [ -n "$dsha$db2" ] || { t_fail "no compressed-runtime digest in srtool-digest.json"; return; }
        [ -z "$dsha" ] || [ "${dsha#0x}" = "$sha" ] || { t_fail "sha256 $sha differs from srtool-digest.json ($dsha)"; return; }
        [ -z "$db2" ] || [ "${db2#0x}" = "$WASM_BLAKE2" ] || { t_fail "blake2-256 differs from srtool-digest.json ($db2)"; return; }
        note="; hashes match srtool-digest.json"
    fi
    printf 'WASM_SOURCE=%q\nWASM_SHA256=%q\nWASM_BLAKE2=%q\n' "$src" "$sha" "0x$WASM_BLAKE2" > "$EV/runtime_wasm.env"
    local verdict=t_pass
    [ -n "$v" ] && [ -z "$note" ] && { verdict=t_warn; note="; not checked: no srtool-digest.json for $L9_REF"; }
    $verdict "$src: $(wc -c < "$WASM" | tr -d ' ') bytes, blake2-256 0x$WASM_BLAKE2$note"
}
t_check HF-WASM-1 "HF-01" "the runtime wasm to enact, checked against the release's srtool digest" c_wasm

# A ledger-8 proof server keeps answering after the upgrade, with DUST proofs the chain
# rejects: the ledger-9 one must be up before the fork.
c_gate_clients() {
    indexer_resolve || { t_fail "no indexer answering"; return; }
    proof_server_up l9 > "$EV/gate_proof_server.log" 2>&1 && proof_server_family_ok "$PS_L9_URL" 9 || { t_fail "no ledger-9 proof server at $PS_L9_URL"; return; }
    local r; r=$(indexer_serves_wallet_fields) || { t_fail "$r"; return; }
    t_pass "proof server $(proof_server_version "$PS_L9_URL") at $PS_L9_URL; the indexer serves the wallet progress fields"
}
c_gate_snapshot() {
    "$SUITE_DIR/checks/snapshot.sh" pre > "${RUN_DIR:-$RUNS_DIR}/snapshot_pre.log" 2>&1 \
        && t_pass "$(table_counts "${RUN_DIR:-$RUNS_DIR}/snapshot_pre.log")" \
        || t_fail "$(table_counts "${RUN_DIR:-$RUNS_DIR}/snapshot_pre.log")"
}
t_check HF-GATE-1 "HF-12" "before set_code: the ledger-9 proof server is up and the indexer serves the fields the wallet SDK needs" c_gate_clients
t_check HF-GATE-2 "HF-01" "the pre-fork snapshot is taken right before set_code" c_gate_snapshot

[ "$T_FAILS" = 0 ] || cannot_run "$T_FAILS check(s) above failed: the runtime upgrade is not enacted"

if [ "${MANUAL_UPGRADE:-0}" = 1 ]; then
    source "$EV/runtime_wasm.env" 2>/dev/null || true
    cat <<MSG

Binary swap done; the runtime upgrade is manual (MANUAL_UPGRADE=1):
  Polkadot.js Apps  https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A9933
  wasm              $WASM
  code hash         ${WASM_BLAKE2:-?} (blake2-256, what system.authorizeUpgrade takes)
  council           ${GOV_COUNCIL_URIS//,/ }, technical committee ${GOV_TC_URIS//,/ }
  motion            the council and the technical committee each pass
                    federatedAuthority.motionApprove(system.authorizeUpgrade(codeHash));
                    then federatedAuthority.motionClose and system.applyAuthorizedUpgrade(code)
  expected          spec $SPEC_BEFORE -> $L9_EXPECTED_SPEC
MSG
    t_finish; exit $?
fi

c_upgrade() {
    # The tool can fail decoding the upgrade block's events after the upgrade was applied:
    # the spec version is the verdict.
    local council tc; IFS=, read -r -a council <<< "$GOV_COUNCIL_URIS"; IFS=, read -r -a tc <<< "$GOV_TC_URIS"
    ( load_compose_env "$L9_ROOT" "$N9" "$T9" >/dev/null && npm run governance-runtime-upgrade:local-env -- \
        --wasm test/midnight_node_runtime.compact.compressed.wasm --rpc-url ws://localhost:9933 \
        --council-uris "${council[@]}" --technical-uris "${tc[@]}" --executor-uri "$GOV_EXECUTOR_URI" --skip-run ) \
        > "$EV/governance_upgrade.log" 2>&1 || t_info "the upgrade tool exited non-zero; checking the spec version"
    local s i
    for i in $(seq 1 36); do s=$(spec_version); [ "$s" != "$SPEC_BEFORE" ] && break; sleep 5; done
    [ "$s" = "$L9_EXPECTED_SPEC" ] && t_pass "spec $SPEC_BEFORE -> $s" || t_fail "spec is $s (expected $L9_EXPECTED_SPEC; evidence/hf/governance_upgrade.log)"
}
c_fork_block() {
    sleep 12
    local fh line n; fh=$(fork_height "$SPEC_BEFORE") || { t_fail "no block where spec $SPEC_BEFORE changes (spec now $(spec_version))"; return; }
    state_set "$FORK_STATE" SPEC_AFTER "$(spec_version)"; state_set "$FORK_STATE" FORK_UTC "$(utc_now)"
    : > "$EV/migration.txt"
    for n in $(log_nodes); do
        node_logs "$n" | grep -oE 'translation complete in [0-9]+ step\(s\), [^,]+, [0-9]+ps synthetic cost' | tail -1 | sed "s/^/$n: /" >> "$EV/migration.txt" || true
    done
    line=$(head -1 "$EV/migration.txt" | cut -d: -f2-)
    t_pass "first block on the new runtime: #$fh;${line:- no translation log line found}"
}
c_fork_agree() {
    state_load "$FORK_STATE"
    local fh; fh=$(block_hash_at "$NODE_HTTP" "$FORK_HEIGHT")
    check_state_agreement "$fh" > "$EV/agreement_fork_block.txt" 2>&1 || { t_fail "fork block #${FORK_HEIGHT:-?}: $(head -1 "$EV/agreement_fork_block.txt")"; return; }
    sweep_forks "$FORK_HEIGHT" "$(node_height)" > "$EV/sweep_after_fork.txt" 2>&1 || { t_fail "$(grep -m1 -E 'FORK|NO AGREEMENT' "$EV/sweep_after_fork.txt")"; return; }
    t_pass "roots agree at #$FORK_HEIGHT; block hashes agree #$FORK_HEIGHT..$(node_height)"
}
t_check HF-UPG-1 "HF-01" "governance set_code of the ledger-9 runtime: spec $SPEC_BEFORE -> $L9_EXPECTED_SPEC" c_upgrade
t_check HF-UPG-2 "HF-01, HF-15" "the fork block found; the ledger translation cost recorded" c_fork_block
t_check HF-UPG-3 "HF-13" "the validators agree at the fork block and on every block after it" c_fork_agree

t_finish
