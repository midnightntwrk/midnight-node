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

# One line of fork status, for `watch -n 60` around the runtime upgrade.
#
# Exit code: 0 the chain has forked (the verified fork height is remembered), 2 not yet,
# 3 block production or finality stalled, 1 the node cannot be asked or cannot show the
# fork block.
set -e
export TARGET=network
# A setup error must not look like exit 2, "not yet".
trap '[ -n "${SETUP_DONE:-}" ] || exit 1' EXIT
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"
SETUP_DONE=1
recall_state

spec=$(spec_version)
[ -n "$spec" ] || { echo "the node at $NODE_HTTP does not answer"; exit 1; }
if health=$(chain_health 20); then healthy=1; else healthy=0; fi
echo "$(utc_now) $NETWORK_NAME: node $(node_version), spec $spec, ledger $(ledger_version), $health"
[ "$healthy" = 1 ] || { echo "block production or finality stalled"; exit 3; }

if [ "$spec" -lt "$NEW_SPEC_FLOOR" ]; then
    echo "not forked yet"
    exit 2
fi
fork_block=$(fork_height "${SPEC_BEFORE_FORK:-$OLD_EXPECTED_SPEC}") \
    || { echo "forked, but the fork block cannot be verified: the RPC node must keep old runtime state"; exit 1; }
echo "forked at block #$fork_block"
