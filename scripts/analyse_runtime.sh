#!/usr/bin/env bash

# Please install subwasm first: https://github.com/chevdor/subwasm

WASM=$1
WASM_FULLPATH=${WASM:="../target/release/wbuild/midnight-node-runtime/midnight_node_runtime.compact.compressed.wasm"}

SZ=`du -s $WASM_FULLPATH | awk '{print $1}'`
PROP=`subwasm -j info $WASM_FULLPATH | jq -r .proposal_hash`
AUTHORIZE_UPGRADE_PROP=`subwasm -j info $WASM_FULLPATH | jq -r .parachain_authorize_upgrade_hash`
MULTIHASH=`subwasm -j info $WASM_FULLPATH | jq -r .ipfs_hash`
SHA256=0x`shasum -a 256 $WASM_FULLPATH | awk '{print $1}'`
BLAKE2_256=`subwasm -j info $WASM_FULLPATH | jq -r .blake2_256`
SUBWASM=`subwasm -j info $WASM_FULLPATH`

JSON=$( jq -n \
    --arg size "$SZ" \
    --arg proposal_hash "$PROP" \
    --arg authorize_upgrade_prop "$AUTHORIZE_UPGRADE_PROP" \
    --arg blake2_256 "$BLAKE2_256" \
    --arg ipfs "$MULTIHASH" \
    --arg sha256 "$SHA256" \
    --arg wasm "$WASM" \
    --argjson subwasm "$SUBWASM" \
    '{
        size: $size,
        proposal_hash: $proposal_hash,
        authorize_upgrade_prop: $authorize_upgrade_prop,
        blake2_256: $blake2_256,
        ipfs: $ipfs,
        sha256: $sha256,
        wasm: $wasm,
        subwasm: $subwasm
    }' )

echo $JSON | jq -cM