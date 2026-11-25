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

# Group 10 on local-env, re-sync: a validator's database is wiped after the fork. It must
# sync from genesis through the fork block and agree with its peers.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/local_env.sh"

WIPED=midnight-node-5      # not validator 1: that one is the reference the others are compared with
WIPED_URL=$(node_url "$WIPED")
evidence_dir g10_wipe_validator
recall_state
require_chain_on new

group_start G10 "Failure and recovery drills on local-env: a wiped validator re-syncs"

check_wiped_validator_resyncs() {
    local started first_height="" best tip attempt finalized block_hash its_ledger_root reference_ledger_root
    docker stop "$WIPED" > /dev/null
    docker run --rm -v "local-env_${WIPED}-data:/data" alpine sh -c 'rm -rf /data/*'
    docker start "$WIPED" > /dev/null
    started=$(date +%s)
    # A low first height proves the database really was empty.
    for attempt in $(seq 1 60); do
        first_height=$(best_height_at "$WIPED_URL")
        [ -n "$first_height" ] && break
        sleep 1
    done
    [ -n "$first_height" ] && [ "$first_height" -lt $(( $(node_height) - 5 )) ] \
        || { fail "$WIPED reported #${first_height:-nothing} right after the restart: its database was not wiped"; return; }
    for attempt in $(seq 1 90); do
        best=$(best_height_at "$WIPED_URL"); tip=$(node_height)
        [ -n "$best" ] && [ -n "$tip" ] && [ "$best" -ge $(( tip - 2 )) ] && break
        sleep 10
    done
    [ -n "$best" ] && [ "$best" -ge $(( tip - 2 )) ] || { fail "$WIPED is at #${best:-unreachable} after 15 minutes, the tip is #$tip"; return; }

    finalized=$(finalized_height); block_hash=$(block_hash_at "$NODE_HTTP" "$finalized")
    [ "$(spec_version_at "$WIPED_URL")" = "$(spec_version)" ] || { fail "$WIPED is on runtime spec $(spec_version_at "$WIPED_URL"), its peers on $(spec_version)"; return; }
    [ -n "$(state_root_at "$WIPED_URL" "$block_hash")" ] && [ "$(state_root_at "$WIPED_URL" "$block_hash")" = "$(state_root_at "$NODE_HTTP" "$block_hash")" ] \
        || { fail "$WIPED synced, but its state root at finalized #$finalized differs from its peers'"; return; }
    its_ledger_root=$(ledger_root_at "$WIPED_URL" "$block_hash"); reference_ledger_root=$(ledger_root_at "$NODE_HTTP" "$block_hash")
    [ "$its_ledger_root" = "$reference_ledger_root" ] && [[ "$its_ledger_root" != ERR:* ]] && [ -n "$its_ledger_root" ] \
        || { fail "$WIPED synced, but the ledger roots at finalized #$finalized differ or are unreadable: $its_ledger_root / $reference_ledger_root"; return; }
    pass "$WIPED restarted empty at #$first_height and synced through the fork block to #$best in $(( $(date +%s) - started ))s; runtime spec, state root and ledger root agree with its peers at finalized #$finalized"
}

bullet "Re-sync: a wiped validator and a wiped indexer sync from genesis through the fork"
check wiped-validator-resyncs "a validator with a wiped database syncs from genesis through the fork and agrees with its peers" check_wiped_validator_resyncs

group_finish
