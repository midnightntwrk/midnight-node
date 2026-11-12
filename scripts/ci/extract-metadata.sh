#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
image=${1:?immutable node image}
[[ "$image" =~ @sha256:[a-f0-9]{64}$ ]] || { echo 'Expected a digest-pinned image' >&2; exit 1; }
docker pull "$image"
cid=$(docker run -d -e CFG_PRESET=dev -p 127.0.0.1::9944 "$image")
result=$(mktemp)
trap 'docker rm -f "$cid" >/dev/null; rm -f "$result"' EXIT
port=$(docker port "$cid" 9944/tcp | head -1)
for _attempt in $(seq 1 60); do
    if curl -fsS --max-time 5 -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":1,"method":"state_getMetadata","params":[]}' \
        "http://$port" > "$result" && jq -e '.result | test("^0x[0-9a-fA-F]+$")' "$result" >/dev/null; then
        version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' node/Cargo.toml | head -1)
        mkdir -p metadata/static
        jq -r .result "$result" | cut -c3- | xxd -r -p > metadata/static/midnight_metadata.scale
        cp metadata/static/midnight_metadata.scale "metadata/static/midnight_metadata_$version.scale"
        exit 0
    fi
    sleep 2
done
echo '::error::Node did not return runtime metadata' >&2
exit 1
