#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail
kind=${1:?main or bot}
case "$kind" in main|bot) ;; *) echo "Invalid plan kind" >&2; exit 1;; esac
sha=$(git rev-parse HEAD)
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' node/Cargo.toml | head -1)
tree=$(git rev-parse 'HEAD^{tree}' | cut -c1-12)
registry="ghcr.io/${GITHUB_REPOSITORY%%/*}"
if [[ "$GITHUB_REPOSITORY" == midnightntwrk/midnight-node ]]; then registry=ghcr.io/midnight-ntwrk; fi
node="$registry/${GITHUB_REPOSITORY#*/}:$version-$tree-amd64"
toolkit="$registry/${GITHUB_REPOSITORY#*/}-toolkit:$version-$tree-amd64"
if [[ -n "${VERSION:-}" ]]; then node="midnightntwrk/midnight-node:$VERSION"; fi
runner='["self-hosted","tier:large"]'
resolve() {
    local ref=$1 digest
    digest=$(docker buildx imagetools inspect "$ref" --format '{{.Manifest.Digest}}') || return 1
    [[ "$digest" =~ ^sha256:[a-f0-9]{64}$ ]] || return 1
    printf '%s@%s' "${ref%:*}" "$digest"
}
node_digest=''; toolkit_digest=''
if [[ "${FORCE_REBUILD:-false}" != true ]] && node_digest=$(resolve "$node"); then
    if [[ "$kind" == bot ]] || toolkit_digest=$(resolve "$toolkit"); then
        runner='["ubuntu-24.04"]'
    fi
fi
if [[ -n "${VERSION:-}" && "$runner" != '["ubuntu-24.04"]' ]]; then
    echo '::error::The explicitly requested image is unavailable; refusing a source-build substitution.' >&2
    exit 1
fi
{
    echo "sha=$sha"
    echo "runner=$runner"
    echo "node_image=$node_digest"
    echo "toolkit_image=$toolkit_digest"
} >> "$GITHUB_OUTPUT"
