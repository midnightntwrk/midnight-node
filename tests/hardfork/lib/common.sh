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

# Paths, the fork under test and portable helpers.

if [ -z "${BASH_VERSINFO:-}" ] || [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
    echo "these scripts need bash >= 4 (macOS: brew install bash and put it first in PATH: the scripts start each other through /usr/bin/env bash)" >&2
    exit 2
fi

SUITE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(git -C "$SUITE_DIR" rev-parse --show-toplevel)"

# --- The fork ------------------------------------------------------------------------
# Everything that differs from one hard fork to the next lives in FORK_DIR: its versions.sh
# (the releases on both sides), its known issues and its own checks.
source "$SUITE_DIR/lib/fork.sh"
IMAGE_REGISTRY="${IMAGE_REGISTRY:-ghcr.io/midnight-ntwrk}"
# shellcheck source=/dev/null
source "$FORK_DIR/versions.sh"
for _v in FORK_NAME OLD_LEDGER NEW_LEDGER OLD_REF OLD_EXPECTED_SPEC NEW_EXPECTED_SPEC NEW_SPEC_FLOOR; do
    [ -n "${!_v:-}" ] || { echo "$_v missing in $FORK_DIR/versions.sh" >&2; exit 2; }
done

release_version() {  # <ref> -> <version> of a node-<version> tag, else empty
    case "$1" in node-*) echo "${1#node-}" ;; *) echo "" ;; esac
}

OLD_NODE_IMAGE_OVERRIDE="${OLD_NODE_IMAGE:-}"; OLD_TOOLKIT_IMAGE_OVERRIDE="${OLD_TOOLKIT_IMAGE:-}"
set_old_images() {
    OLD_NODE_IMAGE="${OLD_NODE_IMAGE_OVERRIDE:-$IMAGE_REGISTRY/midnight-node:$(release_version "$OLD_REF")}"
    OLD_TOOLKIT_IMAGE="${OLD_TOOLKIT_IMAGE_OVERRIDE:-$IMAGE_REGISTRY/midnight-node-toolkit:$(release_version "$OLD_REF")}"
}
set_old_images

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

_new_image() {  # <repository> <override>
    [ -n "$2" ] && { echo "$2"; return; }
    local v; v=$(release_version "${NEW_REF:-}")
    if [ -z "$v" ] && [ ! -f "$NEW_ROOT/node/Cargo.toml" ]; then
        echo "NEW_REF ${NEW_REF:-} is not a node-* release and $NEW_ROOT is not checked out: set NEW_NODE_IMAGE and NEW_TOOLKIT_IMAGE" >&2
        return 1
    fi
    echo "$IMAGE_REGISTRY/$1:${v:-$(ci_image_tag "$NEW_ROOT")}"
}
new_node_image()    { _new_image midnight-node "${NEW_NODE_IMAGE:-}"; }
new_toolkit_image() { _new_image midnight-node-toolkit "${NEW_TOOLKIT_IMAGE:-}"; }

# The two releases are checked out as detached worktrees; the working checkout is never
# switched. Everything the suite writes lives under target/, which git ignores.
HF_WORK_DIR="${HF_WORK_DIR:-$REPO_ROOT/target/hardfork-$FORK_NAME}"
WORKTREES_DIR="${WORKTREES_DIR:-$HF_WORK_DIR/worktrees}"
OLD_ROOT="${OLD_ROOT:-$WORKTREES_DIR/l$OLD_LEDGER}"
NEW_ROOT="${NEW_ROOT:-$WORKTREES_DIR/l$NEW_LEDGER}"

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
# A network's directory must survive between stages: it holds what earlier stages recorded,
# the deployed contracts and the saved old-ledger transaction.
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
    STATE_FILE="$STATE_DIR/state.env"
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
