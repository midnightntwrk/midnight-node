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

# Paths, the release matrix and portable helpers.

if [ -z "${BASH_VERSINFO:-}" ] || [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
    echo "these scripts need bash >= 4 (macOS: brew install bash and put it first in PATH: the scripts start each other through /usr/bin/env bash)" >&2
    exit 2
fi

SUITE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(git -C "$SUITE_DIR" rev-parse --show-toplevel)"

# --- Release matrix -----------------------------------------------------------------
# A node-<version> ref selects the release images, any other ref the CI tree-hash images.
IMAGE_REGISTRY="${IMAGE_REGISTRY:-ghcr.io/midnight-ntwrk}"

L8_REF="${L8_REF:-node-1.0.400}"
L8_EXPECTED_SPEC="${L8_EXPECTED_SPEC:-1000300}"
L8_EXPECTED_LEDGER="${L8_EXPECTED_LEDGER:-=8.1.3}"
L8_EXPECTED_NODE_PREFIX="${L8_EXPECTED_NODE_PREFIX:-1.0.400}"
L8_EXPECTED_COMPACTC="${L8_EXPECTED_COMPACTC:-0.30.0}"

L9_REF="${L9_REF-node-2.1.0-rc.4}"
L9_EXPECTED_SPEC="${L9_EXPECTED_SPEC:-2001000}"
L9_EXPECTED_NODE_PREFIX="${L9_EXPECTED_NODE_PREFIX:-2.1.0}"
# Matched as a substring: the ledger tag naming changed between release candidates.
L9_EXPECTED_LEDGER_SUBSTR="${L9_EXPECTED_LEDGER_SUBSTR:-ledger-9.}"
L9_EXPECTED_LEDGER_TAG="${L9_EXPECTED_LEDGER_TAG:-crate-ledger-9.1.0.0-rc.5}"
L9_EXPECTED_COMPACTC="${L9_EXPECTED_COMPACTC:-0.33.0-rc.1}"

# The indexer checks its compiled-in metadata against the chain and stops at the first
# mismatch, so it must be a build that knows the ledger-9 node.
INDEXER_TAG="${INDEXER_TAG:-4.4.0-rc.6-d543f011}"
INDEXER_RELEASED_TAG="${INDEXER_RELEASED_TAG:-4.4.0-rc.5}"

PS_L8_TAG="${PS_L8_TAG:-8.1.3}"
PS_L9_TAG="${PS_L9_TAG:-9.0.0-rc.7}"

# Ledger-8 specs are below it, ledger-9 specs above.
LEDGER9_SPEC_FLOOR=2000000

release_version() {  # <ref> -> <version> of a node-<version> tag, else empty
    case "$1" in node-*) echo "${1#node-}" ;; *) echo "" ;; esac
}

L8_NODE_IMAGE_OVERRIDE="${L8_NODE_IMAGE:-}"; L8_TOOLKIT_IMAGE_OVERRIDE="${L8_TOOLKIT_IMAGE:-}"
set_l8_images() {
    L8_NODE_IMAGE="${L8_NODE_IMAGE_OVERRIDE:-$IMAGE_REGISTRY/midnight-node:$(release_version "$L8_REF")}"
    L8_TOOLKIT_IMAGE="${L8_TOOLKIT_IMAGE_OVERRIDE:-$IMAGE_REGISTRY/midnight-node-toolkit:$(release_version "$L8_REF")}"
}
set_l8_images

case $(uname -m) in
    x86_64) ARCH="amd64" ;;
    arm64|aarch64) ARCH="arm64" ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 2 ;;
esac

# <node version>-<tree hash 12>-<arch>, as local-environment/.envrc computes it.
ci_image_tag() {  # <checkout>
    local nv tree
    nv=$(grep -m1 '^version' "$1/node/Cargo.toml" | sed 's/version *= *"\([^"]*\)".*/\1/')
    tree=$(git -C "$1" rev-parse 'HEAD^{tree}' | cut -c1-12)
    echo "${nv}-${tree}-${ARCH}"
}

_l9_image() {  # <repository> <override>
    [ -n "$2" ] && { echo "$2"; return; }
    local v; v=$(release_version "${L9_REF:-}")
    if [ -z "$v" ] && [ ! -f "$L9_ROOT/node/Cargo.toml" ]; then
        echo "L9_REF ${L9_REF:-} is not a node-* release and $L9_ROOT is not checked out: set L9_NODE_IMAGE and L9_TOOLKIT_IMAGE" >&2
        return 1
    fi
    echo "$IMAGE_REGISTRY/$1:${v:-$(ci_image_tag "$L9_ROOT")}"
}
l9_node_image()    { _l9_image midnight-node "${L9_NODE_IMAGE:-}"; }
l9_toolkit_image() { _l9_image midnight-node-toolkit "${L9_TOOLKIT_IMAGE:-}"; }

# The two releases are checked out as detached worktrees; the working checkout is never
# switched. Everything the suite writes lives under target/, which git ignores.
HF_WORK_DIR="${HF_WORK_DIR:-$REPO_ROOT/target/hardfork-8-to-9}"
WORKTREES_DIR="${WORKTREES_DIR:-$HF_WORK_DIR/worktrees}"
L8_ROOT="${L8_ROOT:-$WORKTREES_DIR/l8}"
L9_ROOT="${L9_ROOT:-$WORKTREES_DIR/l9}"

ensure_worktree() {  # <path> <ref>; an existing worktree is moved to <ref>
    local wt="$1" ref="$2" want
    [ -n "$ref" ] || { echo "ensure_worktree: no ref for $wt" >&2; return 1; }
    git -C "$REPO_ROOT" fetch -q origin "refs/tags/$ref:refs/tags/$ref" 2>/dev/null \
        || git -C "$REPO_ROOT" fetch -q origin "+refs/heads/$ref:refs/remotes/origin/$ref" 2>/dev/null \
        || git -C "$REPO_ROOT" fetch -q origin "$ref" 2>/dev/null || true
    want=$(git -C "$REPO_ROOT" rev-parse -q --verify "origin/$ref^{commit}" 2>/dev/null \
        || git -C "$REPO_ROOT" rev-parse -q --verify "$ref^{commit}") || { echo "ensure_worktree: unknown ref $ref" >&2; return 1; }
    if [ -e "$wt/.git" ]; then
        [ "$(git -C "$wt" rev-parse HEAD)" = "$want" ] || git -C "$wt" checkout -q --force --detach "$want"
    else
        mkdir -p "$(dirname "$wt")"
        git -C "$REPO_ROOT" worktree add -q --detach "$wt" "$want"
    fi
}

# --- Output -------------------------------------------------------------------------
# A network's directory must survive between stages: it holds the deployed contracts and
# the saved ledger-8 transaction.
out_dir_for() {  # <target> [network]
    if [ "$1" = network ]; then echo "$HF_WORK_DIR/network-${2:?network name}"; else echo "$HF_WORK_DIR/local"; fi
}
init_out_dirs() {
    STATE_DIR="$OUT_DIR/state"
    EVIDENCE_DIR="$OUT_DIR/evidence"
    RESULTS_DIR="$OUT_DIR/results"
    RUNS_DIR="$OUT_DIR/runs"
    DAPPS_DIR="$OUT_DIR/dapps"
    CACHE_DIR="${CACHE_DIR:-$OUT_DIR/cache}"
    mkdir -p "$STATE_DIR" "$EVIDENCE_DIR" "$RESULTS_DIR" "$RUNS_DIR" "$DAPPS_DIR" "$CACHE_DIR"
    PRE_FORK_STATE="$STATE_DIR/pre_fork.env"
    FORK_STATE="$STATE_DIR/fork.env"
    FEATURES_STATE="$STATE_DIR/features.env"
    REGISTRY="$STATE_DIR/contract_registry.tsv"
}

# --- Helpers ------------------------------------------------------------------------
utc_now() { date -u '+%Y-%m-%dT%H:%M:%SZ'; }
hex2dec() { [ -z "$1" ] && echo "" || echo $(( 16#${1#0x} )); }
sed_i() { if sed --version >/dev/null 2>&1; then sed -i "$@"; else sed -i '' "$@"; fi; }
sha256_of() {  # [file], else stdin
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@" | cut -d' ' -f1
    else shasum -a 256 "$@" | cut -d' ' -f1; fi
}
ver_ge() {  # <a> <b>: dotted version a >= b
    python3 -c 'import sys
a, b = ([int(x) for x in v.split(".")] for v in sys.argv[1:3])
sys.exit(0 if a >= b else 1)' "$1" "$2"
}
big_gt() { python3 -c 'import sys; sys.exit(0 if int(sys.argv[1] or 0) > int(sys.argv[2] or 0) else 1)' "$1" "$2"; }
require_cmd() {
    local c missing=""
    for c in "$@"; do command -v "$c" >/dev/null 2>&1 || missing="$missing $c"; done
    [ -z "$missing" ] || { echo "missing required tools:$missing" >&2; return 1; }
}
require_image() {  # <image>: present locally or pullable
    docker image inspect "$1" >/dev/null 2>&1 || docker manifest inspect "$1" >/dev/null 2>&1 \
        || { echo "image $1 not found locally nor in the registry" >&2; return 1; }
}
last_line_of() { [ -f "$1" ] && grep -v '^\s*$' "$1" | tail -1 | cut -c1-"${2:-200}"; }
is_hex64() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }
