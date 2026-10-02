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

# Compile, deploy and call the Compact dApps.
#
# Before the fork, to give the migration varied state: counter, bboard (its take_down()
# needs ledger and private state) and micro-dao (the richest ledger shape, deploy only).
# After the fork, with ledger-9 features: events, keccak (ZKIR v3) and ccc-outer (calls events).
#
# compactc runs inside a toolkit image, so it matches the runtime that image proves with.
# DAPP_OUT picks the artefacts: out (deployed with), out-l9 (recompiled), out-mjs / out-mjs9 (Midnight.js with the
# ledger-8 / ledger-9 compactc).

DAPPS_PRE=(counter bboard)
DAPPS_PRE_BEST_EFFORT=(micro-dao)
DAPP_FILE_PREFIX="${DAPP_FILE_PREFIX:-hf}"
COMPACT_REPO_FALLBACK="${COMPACT_REPO_FALLBACK:-LFDT-Minokawa/compact}"

dapp_dir()   { echo "$DAPPS_DIR/$1"; }
dapp_out()   { echo "${DAPP_OUT:-out}"; }
dapp_cfg()   { if [ "$(dapp_out)" = out ]; then echo "contract.config.ts"; else echo "contract.$(dapp_out).config.ts"; fi; }
dapp_image() { echo "${DAPP_IMAGE:-$TK_IMAGE}"; }
dapp_file()  { echo "$DAPPS_DIR/${DAPP_FILE_PREFIX}_$1"; }
dapp_log()   { echo "$(dapp_dir "$1")/${DAPP_FILE_PREFIX}_$2.log"; }  # <name> <step>
dapp_tail()  { tail -n 3 "$(dapp_log "$1" "$2")" 2>/dev/null | tr '\n' ' ' | cut -c1-240; }

dapp_compactc_flags() { case "$1" in keccak) echo "--feature-zkir-v3" ;; *) echo "" ;; esac; }

dapp_compactc_version() {  # <toolkit-image>
    docker run --rm "$1" version 2>/dev/null | grep -i '^Compactc:' | awk '{print $2}'
}

# A compiler the image does not bundle, fetched on the host: the images have no unzip, so
# their fetch-compactc cannot extract a release archive.
compactc_to_cache() {  # <version>
    local v=$1 dir="$CACHE_DIR/compactc/$1" arch repo url zip
    [ -x "$dir/compactc" ] && return 0
    case "$ARCH" in arm64) arch=aarch64 ;; *) arch=x86_64 ;; esac
    zip="$CACHE_DIR/compactc/compactc_v${v}.zip"; mkdir -p "$dir"
    for repo in midnightntwrk/compact "$COMPACT_REPO_FALLBACK"; do
        url="https://github.com/$repo/releases/download/compactc-v$v/compactc_v${v}_${arch}-unknown-linux-musl.zip"
        curl -sfL -o "$zip" "$url" && break
    done
    [ -s "$zip" ] || { echo "compactc $v: no linux release archive found" >&2; return 1; }
    python3 -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$zip" "$dir" && rm -f "$zip"
    # A release archive may nest its files one directory down.
    if [ ! -e "$dir/compactc" ]; then local sub; sub=$(find "$dir" -name compactc -type f | head -1); [ -n "$sub" ] && mv "$(dirname "$sub")"/* "$dir/"; fi
    chmod -R +x "$dir"
    [ -e "$dir/compactc" ]
}

dapp_compile() {  # <name> <image>
    local name=$1 img=$2 d ver out
    d="$(dapp_dir "$name")"; out="$(dapp_out)"
    mkdir -p "$d" "$CACHE_DIR/compactc"
    cp "$SUITE_DIR/dapps/$name.compact" "$d/$name.compact"
    ver="${DAPP_COMPACTC_VERSION:-$(dapp_compactc_version "$img")}"
    [ -n "$ver" ] || { echo "cannot read the compactc version of $img" >&2; return 1; }
    echo "$ver" > "$d/compactc.$out.version"
    if [ -d "$d/$out/keys" ] && [ "$(cat "$d/$out/.compactc" 2>/dev/null)" = "$ver" ]; then
        dapp_post_compile "$name"; return
    fi
    rm -rf "${d:?}/$out"
    [ "$ver" = "$(dapp_compactc_version "$img")" ] || compactc_to_cache "$ver" || return 1
    docker run --rm --entrypoint sh -e "V=$ver" -e "SRC=$name.compact" -e "OUT=$out" \
        -e "FLAGS=$(dapp_compactc_flags "$name")" -e "OWNER=$(id -u):$(id -g)" \
        -v "$d:/work" -v "$CACHE_DIR/compactc:/compactc-cache" "$img" -c '
        set -e
        managed=$(dirname "$(dirname "$(readlink -f /toolkit-js/node_modules/.bin/fetch-compactc)")")/managed
        mkdir -p "$managed"
        [ -e "/compactc-cache/$V/compactc" ] && { rm -rf "${managed:?}/$V"; cp -R "/compactc-cache/$V" "$managed/"; }
        cd /toolkit-js
        COMPACTC_VERSION="$V" npx run-compactc $FLAGS "/work/$SRC" "/work/$OUT"
        chown -R "$OWNER" "/work/$OUT"' > "$d/compile.$out.log" 2>&1 || return 1
    echo "$ver" > "$d/$out/.compactc"
    dapp_fix_dts "$name"
    dapp_post_compile "$name"
}

# compact-js parses CLI arguments from index.d.ts and knows only inline struct types, so
# micro-dao's type aliases are inlined.
dapp_fix_dts() {
    local dts; dts="$(dapp_dir "$1")/$(dapp_out)/contract/index.d.ts"
    [ -f "$dts" ] || return 0
    sed_i -e 's/costs_param_0: Costs)/costs_param_0: { seed_dust: bigint; buy_in_dust: bigint })/' \
        -e 's/\(_0: \)ShieldedCoinInfo\([,)]\)/\1{ nonce: Uint8Array; color: Uint8Array; value: bigint }\2/g' "$dts"
}

# ccc-outer imports the deployed callee's artefacts from <dir>/Events.
dapp_post_compile() {
    case "$1" in
    ccc-outer)
        local callee; callee="$(dapp_dir events)/out"
        [ -d "$callee/contract" ] || { echo "ccc-outer needs the compiled events dApp first" >&2; return 1; }
        rm -rf "$(dapp_dir ccc-outer)/Events" && cp -R "$callee" "$(dapp_dir ccc-outer)/Events" ;;
    esac
    return 0
}

dapp_write_config() {  # <name> <coin-public>
    local name=$1 cp=$2 d tmpl cname="" out; d="$(dapp_dir "$name")"; out="$(dapp_out)"
    case "$name" in
        counter|bboard|micro-dao) tmpl="$SUITE_DIR/dapps/config/$name.config.ts.tmpl" ;;
        events) tmpl="$SUITE_DIR/dapps/config/vacant.config.ts.tmpl"; cname=EventsContract ;;
        keccak) tmpl="$SUITE_DIR/dapps/config/vacant.config.ts.tmpl"; cname=KeccakContract ;;
        ccc-outer) tmpl="$SUITE_DIR/dapps/config/vacant.config.ts.tmpl"; cname=CccOuterContract ;;
        *) echo "no config template for $name" >&2; return 1 ;;
    esac
    sed -e "s#__OUT__#$out#g" -e "s#__COIN_PUBLIC__#$cp#g" -e "s#__NETWORK_ID__#$NETWORK_ID#g" \
        -e "s#__CONTRACT_NAME__#$cname#g" "$tmpl" > "$d/$(dapp_cfg)"
}

dapp_ctor_args() {  # <name>
    case "$1" in
    micro-dao) echo "deadbeefcafebabe1234567890abcdef1122334455667788aabbccddeeff0011 {\"seed_dust\":1000000,\"buy_in_dust\":1000000}" ;;
    ccc-outer)
        : "${CCC_CALLEE_ADDRESS:?CCC_CALLEE_ADDRESS must name the events contract}"
        # A contract parameter is { bytes: Uint8Array }; CCC_CTOR_STYLE=hex passes bare hex.
        if [ "${CCC_CTOR_STYLE:-json}" = hex ]; then echo "$CCC_CALLEE_ADDRESS"; else echo "{\"bytes\":\"$CCC_CALLEE_ADDRESS\"}"; fi ;;
    *) echo "" ;;
    esac
}

# The dApp is mounted inside /toolkit-js, so Node resolves compact-js from the image.
dapp_js() {  # <name> <args...>
    local name=$1; shift
    docker run --rm --network host -e "RESTORE_OWNER=$(id -u):$(id -g)" \
        -v "$(dapp_dir "$name"):/toolkit-js/contract" -v "$DAPPS_DIR:$DAPPS_DIR" "$(dapp_image)" "$@"
}

# Prove <stem>.bin into <stem>_tx.mn and submit it; 1 when proving failed, 2 when sending.
_dapp_prove_send() {  # <name> <funding-seed> <stem> <log-step>
    local tx; tx="$(dapp_file "${3}_tx.mn")"
    tk_chain send-intent --intent-file "$(dapp_file "$3.bin")" --compiled-contract-dir "$(dapp_dir "$1")/$(dapp_out)" \
        --funding-seed "$2" --dest-file "$tx" > "$(dapp_log "$1" "${4}_prove")" 2>&1 || return 1
    tk_send "$tx" > "$(dapp_log "$1" "${4}_send")" 2>&1 || return 2
}

dapp_deploy() {  # <name> <funding-seed> -> contract address
    local name=$1 seed=$2 cp addr args rc=0
    dapp_compile "$name" "$(dapp_image)" || { echo "COMPILE_FAILED"; return 1; }
    cp=$(tk_address "$seed" --coin-public)
    dapp_write_config "$name" "$cp" || { echo "CONFIG_FAILED"; return 1; }
    read -r -a args <<< "$(dapp_ctor_args "$name")"
    dapp_js "$name" generate-intent deploy -c "/toolkit-js/contract/$(dapp_cfg)" --network "$NETWORK_ID" \
        --coin-public "$cp" --output-intent "$(dapp_file "${name}_deploy.bin")" \
        --output-private-state "$(dapp_file "${name}_private_state.json")" \
        --output-zswap-state "$(dapp_file "${name}_zswap.json")" "${args[@]}" \
        > "$(dapp_log "$name" deploy_intent)" 2>&1 || { echo "INTENT_FAILED"; return 1; }
    _dapp_prove_send "$name" "$seed" "${name}_deploy" deploy || rc=$?
    case $rc in 1) echo "PROVE_FAILED"; return 1 ;; 2) echo "SEND_FAILED"; return 1 ;; esac
    addr=$(tk contract-address --src-file "$(dapp_file "${name}_deploy_tx.mn")" 2>/dev/null | tail -1)
    is_hex64 "$addr" || { echo "ADDRESS_FAILED"; return 1; }
    echo "$addr"
}

# DAPP_EVENTS_OUT=1 and DAPP_RESULT_OUT=1 save the emitted events and the return value.
dapp_call() {  # <name> <funding-seed> <address> <circuit> [args...]
    local name=$1 seed=$2 addr=$3 circuit=$4 cp extra=(); shift 4
    [ -n "${DAPP_EVENTS_OUT:-}" ] && extra+=(--output-events "$(dapp_file "${name}_events.json")")
    [ -n "${DAPP_RESULT_OUT:-}" ] && extra+=(--output-result "$(dapp_file "${name}_result.json")")
    cp=$(tk_address "$seed" --coin-public)
    tk_chain_ro contract-state --contract-address "$addr" --dest-file "$(dapp_file "${name}_state.mn")" \
        > "$(dapp_log "$name" "${circuit}_state")" 2>&1 || return 1
    dapp_js "$name" generate-intent circuit -s "$NODE_WS" -c "/toolkit-js/contract/$(dapp_cfg)" --network "$NETWORK_ID" \
        --coin-public "$cp" --contract-address "$addr" \
        --input-onchain-state "$(dapp_file "${name}_state.mn")" \
        --input-private-state "$(dapp_file "${name}_private_state.json")" \
        --output-intent "$(dapp_file "${name}_call.bin")" \
        --output-private-state "$(dapp_file "${name}_private_state.json")" \
        --output-zswap-state "$(dapp_file "${name}_call_zswap.json")" \
        "${extra[@]}" "$circuit" "$@" > "$(dapp_log "$name" "${circuit}_intent")" 2>&1 || return 1
    _dapp_prove_send "$name" "$seed" "${name}_call" "$circuit"
}

# Recompile into out-l9 and compare verifier keys: identical keys mean the on-chain keys
# accept the new proofs. Returns 1 when a key differs, 2 when the compile failed.
dapp_recompile() {  # <name> <image> <coin-public>
    local name=$1 img=$2 cp=$3 d k rc=0; d="$(dapp_dir "$name")"
    DAPP_OUT=out-l9 dapp_compile "$name" "$img" || { echo "recompile failed ($d/compile.out-l9.log)"; return 2; }
    DAPP_OUT=out-l9 dapp_write_config "$name" "$cp" || return 2
    echo "compactc $(cat "$d/out/.compactc") -> $(cat "$d/out-l9/.compactc")"
    for k in "$d"/out/keys/*.verifier; do
        [ -f "$k" ] || continue
        if cmp -s "$k" "$d/out-l9/keys/$(basename "$k")"; then echo "$(basename "$k" .verifier): identical verifier key"
        else echo "$(basename "$k" .verifier): different verifier key"; rc=1; fi
    done
    return $rc
}
