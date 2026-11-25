#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Group 1: release artefacts verification.
#
# Are the builds about to be deployed the ones that were tested? Inspects git tags, images,
# npm packages and the runtime wasm. Sends nothing to the chain, so it can be repeated at any
# stage, for instance when operators change an image late.
#
# The fork adds its dependency pins and the regressions of its security fixes
# (forks/<fork>/<this file>).
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

evidence_dir g1
indexer_resolve || true

OLD_NODE_REF="$OLD_REF"
NEW_NODE_REF="${NEW_REF:-$(git -C "$NEW_ROOT" rev-parse HEAD 2>/dev/null || true)}"
NEW_NODE_IMAGE_UNDER_TEST="${DEPLOYED_NODE_IMAGE:-$(new_node_image)}"
NEW_TOOLKIT_IMAGE_UNDER_TEST="$(new_toolkit_image)"
git -C "$REPO_ROOT" fetch -q origin "refs/tags/$OLD_REF:refs/tags/$OLD_REF" 2>/dev/null || true
[ -z "${NEW_REF:-}" ] || git -C "$REPO_ROOT" fetch -q origin "refs/tags/$NEW_REF:refs/tags/$NEW_REF" 2>/dev/null || true

# The indexer's source commit: named in the network settings, read from a CI image tag
# (<version>-<commit>), or the release tag.
INDEXER_SOURCE="${INDEXER_SOURCE_COMMIT:-}"
if [ -z "$INDEXER_SOURCE" ]; then
    if [[ "${INDEXER_TAG##*-}" =~ ^[0-9a-f]{8}$ ]]; then INDEXER_SOURCE="${INDEXER_TAG##*-}"; else INDEXER_SOURCE="v$INDEXER_TAG"; fi
fi

group_start G1 "Release artefacts verification on $NETWORK_NAME: node $OLD_NODE_REF -> ${NEW_NODE_REF:-?}, indexer $INDEXER_TAG"

# --- The runtime wasm --------------------------------------------------------------------
# A release ships the runtime that governance enacts, with a digest of its reproducible
# build. The wasm inside the node image is not that file, so the release asset is what is
# checked and what local-env enacts (state/runtime.wasm).
RUNTIME_WASM="$STATE_DIR/runtime.wasm"
blake2_256_of() { python3 -c "import hashlib, sys; print(hashlib.blake2b(open(sys.argv[1], 'rb').read(), digest_size=32).hexdigest())" "$1"; }
digest_of_compressed_runtime() {  # <sha256|blake2_256> <srtool-digest.json>
    jq -r --arg hash "$1" '.. | objects | select(.compressed | type == "object") | .compressed[$hash] // empty' "$2" | head -1
}

check_runtime_wasm_matches_published_digest() {
    local download="$EV/release" asset digest_file sha256 blake2 published_sha256 published_blake2
    [ -n "$(release_version "${NEW_REF:-}")" ] || { skip "${NEW_REF:-no ref} is not a release: there is no published digest to compare with"; return; }
    rm -rf "$download"; mkdir -p "$download"
    gh release download "$NEW_REF" -R midnightntwrk/midnight-node -p '*.compact.compressed.wasm' -p 'srtool-digest.json' \
        -D "$download" --clobber > "$EV/release_download.log" 2>&1 || { fail "could not download the release assets of $NEW_REF (is gh logged in?): $(last_line_of "$EV/release_download.log")"; return; }
    # The runtime's version is not always the node's, so the asset is found by pattern.
    asset=$(ls "$download"/midnight_node_runtime-*.compact.compressed.wasm 2>/dev/null | head -1)
    digest_file="$download/srtool-digest.json"
    [ -s "$asset" ] || { fail "$NEW_REF has no runtime wasm among its release assets"; return; }
    [ -s "$digest_file" ] || { fail "$NEW_REF has no srtool-digest.json among its release assets"; return; }
    sha256=$(sha256_of "$asset"); blake2=$(blake2_256_of "$asset")
    published_sha256=$(digest_of_compressed_runtime sha256 "$digest_file"); published_blake2=$(digest_of_compressed_runtime blake2_256 "$digest_file")
    [ -n "$published_sha256$published_blake2" ] || { fail "srtool-digest.json names no digest of the compressed runtime"; return; }
    [ -z "$published_sha256" ] || [ "${published_sha256#0x}" = "$sha256" ] || { fail "sha256 $sha256 differs from the published $published_sha256"; return; }
    [ -z "$published_blake2" ] || [ "${published_blake2#0x}" = "$blake2" ] || { fail "blake2-256 0x$blake2 differs from the published $published_blake2"; return; }
    cp "$asset" "$RUNTIME_WASM"
    remember RUNTIME_WASM_BLAKE2 "0x$blake2"
    pass "$(basename "$asset"), $(wc -c < "$asset" | tr -d ' ') bytes: sha256 and blake2-256 (0x$blake2) match srtool-digest.json"
}

# --- The fixes the release must carry ------------------------------------------------------
# Compared by commit, not by version label: a build can report an older version and still
# contain the fix. required-fixes.tsv: component, commit, reference, description.
commit_is_in_node_ref() {  # <commit> <ref>
    git -C "$REPO_ROOT" cat-file -e "$1^{commit}" 2>/dev/null && git -C "$REPO_ROOT" merge-base --is-ancestor "$1" "$2" 2>/dev/null
}

commit_is_in_indexer() {  # <commit> -> yes, no or unknown
    local comparison status
    comparison="repos/midnightntwrk/midnight-indexer/compare/$1...$INDEXER_SOURCE"
    # Through gh when it is logged in: anonymous GitHub API calls are limited to 60 an hour.
    if gh auth status > /dev/null 2>&1; then status=$(gh api "$comparison" --jq '.status' 2>/dev/null)
    else status=$(curl -s -m 20 "https://api.github.com/$comparison" | jq -r '.status // empty'); fi
    case "$status" in
        ahead|identical) echo yes ;;
        "") echo unknown ;;
        *) echo no ;;
    esac
}

REQUIRED_FIXES_REPORT="$EV/required_fixes.md"
printf '| Component | Commit | Reference | Description | In the build |\n|---|---|---|---|---|\n' > "$REQUIRED_FIXES_REPORT"

check_required_fixes() {  # <node-old|node-new|indexer>
    local wanted=$1 component commit reference description present found=0 missing="" unverified=""
    while IFS=$'\t' read -r component commit reference description; do
        [ "$component" = "$wanted" ] || continue
        case "$wanted" in
            node-old) if commit_is_in_node_ref "$commit" "$OLD_NODE_REF"; then present=yes; else present=no; fi ;;
            node-new)
                if [ -z "$NEW_NODE_REF" ]; then present=unknown
                elif commit_is_in_node_ref "$commit" "$NEW_NODE_REF"; then present=yes
                else present=no; fi ;;
            indexer) present=$(commit_is_in_indexer "$commit") ;;
        esac
        printf '| %s | %s | %s | %s | %s |\n' "$component" "$commit" "$reference" "$description" "$present" >> "$REQUIRED_FIXES_REPORT"
        case "$present" in
            yes) found=$(( found + 1 )) ;;
            no) missing="$missing $commit ($reference);" ;;
            *) unverified="$unverified $commit" ;;
        esac
    done < <(grep -v '^#' "$FORK_DIR/required-fixes.tsv")
    if [ -n "$missing" ]; then fail "missing:$missing"
    elif [ -n "$unverified" ]; then warn "$found present; could not verify:$unverified"
    elif [ "$found" = 0 ]; then fail "required-fixes.tsv lists no fix for $wanted"
    else pass "$found required fixes present ($REQUIRED_FIXES_REPORT)"; fi
}

# --- The versions of the release matrix ------------------------------------------------------
check_proof_servers_are_release_versions() {
    local old_version new_version
    proof_server_up old > "$EV/proof_server_old.log" 2>&1 || true
    proof_server_up new > "$EV/proof_server_new.log" 2>&1 || true
    old_version=$(proof_server_version "$PROOF_SERVER_OLD_URL"); new_version=$(proof_server_version "$PROOF_SERVER_NEW_URL")
    if [ -z "$old_version$new_version" ]; then fail "no proof server could be started or reached"
    elif [ "$old_version" = "$PROOF_SERVER_OLD_TAG" ] && [ "$new_version" = "$PROOF_SERVER_NEW_TAG" ]; then pass "$old_version and $new_version"
    else fail "they report '$old_version' and '$new_version', the release matrix says $PROOF_SERVER_OLD_TAG and $PROOF_SERVER_NEW_TAG"; fi
}

# --- The group ----------------------------------------------------------------------------
bullet "The runtime wasm matches its published digest"
check runtime-wasm-matches-published-digest "the runtime wasm of $NEW_REF against the digest published with it" check_runtime_wasm_matches_published_digest

bullet "The fixes the release must carry are really in the deployed tags"
check required-fixes-in-old-node  "every required fix is in the old node tag, $OLD_NODE_REF"            check_required_fixes node-old
check required-fixes-in-new-node  "every required fix is in the new node ref, ${NEW_NODE_REF:-?}"       check_required_fixes node-new
check required-fixes-in-indexer   "every required fix is in the indexer's source, $INDEXER_SOURCE"      check_required_fixes indexer

bullet "The versions are those of the release matrix"
check proof-servers-are-release-versions "the two proof servers report the versions of the release matrix" check_proof_servers_are_release_versions

run_fork_checks

group_finish
