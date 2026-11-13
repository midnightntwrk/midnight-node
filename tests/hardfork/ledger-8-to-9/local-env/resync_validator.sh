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

# Table HF11: a wiped validator re-syncs through the fork and agrees with its peers.
#
#   resync_validator.sh [node]     (default midnight-node-5)
set -e
source "$(dirname "${BASH_SOURCE[0]}")/lib_local.sh"

VICTIM="${1:-midnight-node-5}"
URL=$(node_url "$VICTIM") || cannot_run "unknown node $VICTIM"
[ "$URL" != "$NODE_HTTP" ] || cannot_run "$VICTIM serves NODE_HTTP, the reference it would be compared with"
require_era l9

t_table HF11 "A wiped validator ($VICTIM) re-syncs genesis -> fork -> tip"

c_resync() {
    docker stop "$VICTIM" >/dev/null
    docker run --rm -v "local-env_${VICTIM}-data:/data" alpine sh -c 'rm -rf /data/*'
    docker start "$VICTIM" >/dev/null
    local t0 b tip i b0=""; t0=$(date +%s)
    # A low first height proves the database really was empty.
    for i in $(seq 1 60); do b0=$(best_height_at "$URL"); [ -n "$b0" ] && break; sleep 1; done
    [ -n "$b0" ] && [ "$b0" -lt $(( $(node_height) - 5 )) ] || { t_fail "$VICTIM reported #${b0:-none} right after the restart: not wiped"; return; }
    for i in $(seq 1 90); do
        b=$(best_height_at "$URL"); tip=$(node_height)
        [ -n "$b" ] && [ -n "$tip" ] && [ "$b" -ge $(( tip - 2 )) ] && break
        sleep 10
    done
    [ -n "$b" ] && [ "$b" -ge $(( tip - 2 )) ] && t_pass "restarted at #$b0, synced to #$b (tip $tip) in $(( $(date +%s) - t0 )) s" || t_fail "at ${b:-unreachable}, tip $tip after 15 min"
}
c_agree() {
    local fin bh; fin=$(finalized_height); bh=$(block_hash_at "$NODE_HTTP" "$fin")
    [ "$(spec_version_at "$URL")" = "$(spec_version)" ] || { t_fail "spec $(spec_version_at "$URL") vs $(spec_version)"; return; }
    [ -n "$(state_root_at "$URL" "$bh")" ] && [ "$(state_root_at "$URL" "$bh")" = "$(state_root_at "$NODE_HTTP" "$bh")" ] || { t_fail "stateRoot differs at #$fin"; return; }
    local mine ref; mine=$(ledger_root_at "$URL" "$bh"); ref=$(ledger_root_at "$NODE_HTTP" "$bh")
    case "$mine$ref" in *ERR:*|"") t_warn "stateRoot agrees at #$fin; the ledger root RPC is unavailable ($mine / $ref)"; return ;; esac
    [ "$mine" = "$ref" ] && t_pass "spec, stateRoot and ledger root agree at finalized #$fin" || t_fail "the ledger root differs at #$fin: $mine vs $ref"
}
t_check HF11-1 "HF-11" "$VICTIM wiped, restarted, synced to the tip" c_resync
t_check HF11-2 "HF-11" "the re-synced node agrees with its peers at the finalized block" c_agree
t_finish
