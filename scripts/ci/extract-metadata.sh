#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
image=${1:?immutable node image}
output=${2:-metadata/static}
[[ "$image" =~ @sha256:[a-f0-9]{64}$ ]] || { echo 'Expected a digest-pinned image' >&2; exit 1; }
docker pull "$image"
cid=$(docker run -d -e CFG_PRESET=dev -p 127.0.0.1::9944 "$image")
trap 'docker rm -f "$cid" >/dev/null' EXIT
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
port=$(docker port "$cid" 9944/tcp | head -1)
for _attempt in $(seq 1 60); do
    if curl -fsS --max-time 5 "http://$port/health" >/dev/null; then
        version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' node/Cargo.toml | head -1)
        mkdir -p "$output"
        python3 "$script_dir/fetch_metadata.py" "http://$port" "$output/midnight_metadata.scale"
        cp "$output/midnight_metadata.scale" "$output/midnight_metadata_$version.scale"
        exit 0
    fi
    sleep 2
done
echo '::error::Node did not return runtime metadata' >&2
exit 1
