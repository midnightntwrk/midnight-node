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

# Group 1, the ledger 8 -> 9 fork's part: the dependency pins this release must have, and
# the regressions of the security fixes it carries.
# Sourced by groups/g01_release_artefacts.sh, which sets OLD_NODE_REF, NEW_NODE_REF and the
# images under test.

# --- Dependency pins --------------------------------------------------------------------
old_ledger_pin_of() {  # <git ref>: the mn-ledger-8 version its Cargo.toml pins
    git -C "$REPO_ROOT" show "$1:Cargo.toml" 2>/dev/null | grep -m1 'mn-ledger-8 *=' | grep -o 'version = "[^"]*"' | cut -d'"' -f2
}

# During the waves both binaries apply ledger-8 blocks, so they must use the same ledger 8.
check_both_nodes_pin_same_old_ledger() {
    local old_pin new_pin
    old_pin=$(old_ledger_pin_of "$OLD_NODE_REF"); new_pin=$(old_ledger_pin_of "$NEW_NODE_REF")
    if [ "$old_pin" = "$OLD_EXPECTED_LEDGER" ] && [ "$new_pin" = "$OLD_EXPECTED_LEDGER" ]; then pass "both pin mn-ledger-8 $old_pin"
    else fail "$OLD_NODE_REF pins '$old_pin', $NEW_NODE_REF pins '$new_pin', expected $OLD_EXPECTED_LEDGER in both"; fi
}

check_new_node_pins_intended_ledger() {
    local pin
    pin=$(git -C "$REPO_ROOT" show "$NEW_NODE_REF:Cargo.lock" 2>/dev/null \
        | grep -A2 '^name = "midnight-ledger-v9"$' | grep -o 'tag=[^#]*' | head -1 | cut -d= -f2)
    if [ "$pin" = "$NEW_EXPECTED_LEDGER_TAG" ]; then pass "Cargo.lock pins midnight-ledger-v9 at $pin"
    else fail "Cargo.lock pins '$pin', expected $NEW_EXPECTED_LEDGER_TAG"; fi
}

check_new_node_pins_intended_polkadot_sdk() {
    local branches
    branches=$(git -C "$REPO_ROOT" show "$NEW_NODE_REF:Cargo.toml" 2>/dev/null \
        | grep -oE 'polkadot-stable[0-9]+|branch = "stable[0-9]+"' | sort -u | tr '\n' ' ')
    if [[ "$branches" == *2606* && "$branches" != *2603* ]]; then pass "$branches"
    else fail "$NEW_NODE_REF pins '$branches', expected stable2606 only"; fi
}

# The wallet SDK and Midnight.js each bundle a ledger build; a build other than the chain's
# can make transactions the chain refuses.
check_clients_bundle_the_chain_ledger() {
    clients_ready > "$EV/clients_ready.log" 2>&1 || { fail "npm ci in $CLIENT_PACKAGES_DIR failed ($EV/clients_ready.log)"; return; }
    local installed old_build new_build
    installed=$(clients_versions)
    old_build=$(client_version ledger-v8 "$installed"); new_build=$(client_version ledger-v9 "$installed")
    if [ "=$old_build" != "$OLD_EXPECTED_LEDGER" ]; then
        fail "the clients bundle ledger-v8 $old_build, the old chain runs $OLD_EXPECTED_LEDGER"
    elif [ "$new_build" != "$NEW_EXPECTED_LEDGER" ]; then
        warn "the clients bundle ledger-v9 $new_build, which Midnight.js $(client_version midnight-js "$installed") pins; the new chain runs $NEW_EXPECTED_LEDGER"
    else
        pass "the clients bundle ledger-v8 $old_build and ledger-v9 $new_build"
    fi
}

# --- Security fixes ---------------------------------------------------------------------
image_has_gdb() {  # <image> -> present or absent
    docker run --rm --entrypoint sh "$1" -c 'command -v gdb >/dev/null 2>&1 && echo present || echo absent' 2>/dev/null | tail -1
}

# A debugger in a production image helps an attacker who got in (node-1.0.300 release notes).
check_images_ship_no_debugger() {
    local image answer report="" with_gdb=""
    for image in "$OLD_NODE_IMAGE" "$OLD_TOOLKIT_IMAGE" "$NEW_NODE_IMAGE_UNDER_TEST" "$NEW_TOOLKIT_IMAGE_UNDER_TEST"; do
        answer=$(image_has_gdb "$image")
        report="$report ${image##*/}: gdb $answer;"
        [ "$answer" = absent ] || with_gdb="$with_gdb ${image##*/}"
    done
    if [ -z "$with_gdb" ]; then pass "$report"
    else fail "gdb present, or the image unreadable:$with_gdb ($report)"; fi
}

npm_tar_version_in() {  # <image>
    docker run --rm --entrypoint sh "$1" \
        -c 'grep -m1 "\"version\"" "$(npm root -g)/npm/node_modules/tar/package.json" 2>/dev/null | tr -dc 0-9.' 2>/dev/null | tail -1
}

# GHSA-23hp-3jrh-7fpw: the tar package bundled with npm, fixed in 7.5.19.
check_toolkit_images_have_patched_tar() {
    local old_tar new_tar
    old_tar=$(npm_tar_version_in "$OLD_TOOLKIT_IMAGE"); new_tar=$(npm_tar_version_in "$NEW_TOOLKIT_IMAGE_UNDER_TEST")
    if [ -n "$old_tar" ] && [ -n "$new_tar" ] && ver_ge "$old_tar" 7.5.19 && ver_ge "$new_tar" 7.5.19; then
        pass "old toolkit: tar $old_tar; new toolkit: tar $new_tar"
    else fail "tar '${old_tar:-unreadable}' and '${new_tar:-unreadable}', both must be 7.5.19 or later"; fi
}

# midnight-indexer#1455: a query over an unbounded epoch range could exhaust the API.
check_indexer_refuses_unbounded_epoch_range() {
    [ -n "$INDEXER_GQL" ] || { skip "no indexer answers at $INDEXER_BASE"; return; }
    local unbounded_error bounded
    unbounded_error=$(gql_query '{ registeredTotalsSeries(fromEpoch: 0, toEpoch: 9223372036854775807) { __typename } }' | jq -r '.errors[0].message // empty')
    if gql_query '{ registeredTotalsSeries(fromEpoch: 0, toEpoch: 10) { __typename } }' | jq -e '.data.registeredTotalsSeries != null' > /dev/null 2>&1; then
        bounded=answered
    else bounded=failed; fi
    if [[ "$unbounded_error" == *"epoch range too large"* ]] && [ "$bounded" = answered ]; then
        pass "the whole i64 range is refused ('$unbounded_error'); a range of 10 epochs is answered"
    elif [[ "$unbounded_error" == *"Unknown field"* || "$unbounded_error" == *"Cannot query field"* ]]; then
        skip "this indexer API has no registeredTotalsSeries query"
    else
        fail "the whole i64 range: '${unbounded_error:-no error}'; a range of 10 epochs: $bounded"
    fi
}

bullet "The dependency pins are the intended ones"
check both-nodes-pin-same-old-ledger      "both node releases pin ledger $OLD_EXPECTED_LEDGER"                    check_both_nodes_pin_same_old_ledger
check new-node-pins-intended-ledger       "the new node pins $NEW_EXPECTED_LEDGER_TAG"                           check_new_node_pins_intended_ledger
check new-node-pins-intended-polkadot-sdk "the new node builds on polkadot-sdk stable2606"                       check_new_node_pins_intended_polkadot_sdk
check clients-bundle-the-chain-ledger     "the wallet SDK and Midnight.js bundle the ledger builds the chain runs" check_clients_bundle_the_chain_ledger

bullet "Security fixes of this release"
check images-ship-no-debugger             "no gdb in the node and toolkit images of both releases"               check_images_ship_no_debugger
check toolkit-images-have-patched-tar     "npm's bundled tar is 7.5.19 or later in both toolkit images"          check_toolkit_images_have_patched_tar
check indexer-refuses-unbounded-epoch-range "the indexer API refuses an unbounded epoch range and answers a bounded one" check_indexer_refuses_unbounded_epoch_range
