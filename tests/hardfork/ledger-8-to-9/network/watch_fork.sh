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

# Fork status, for `watch -n 60 network/watch_fork.sh` around the runtime upgrade.
# Exits 0 once forked (records the verified fork height), 2 before, 3 when production or
# finality stalls, 1 on a setup error or when the RPC is unreachable or cannot show the fork block.
set -e
export TARGET=network
trap '[ -n "${READY:-}" ] || exit 1' EXIT   # a setup error must not read as exit 2, "pre-fork"
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"
READY=1
s=$(spec_version); [ -n "$s" ] || { echo "RPC $NODE_HTTP unreachable"; exit 1; }
health=$(chain_health 20) && ok=1 || ok=0
echo "$(utc_now) $NETWORK_NAME: node $(node_version), spec $s, ledger $(ledger_version), $health"
[ "$ok" = 1 ] || { echo "finality or production stalled"; exit 3; }
if [ "$s" -ge "$LEDGER9_SPEC_FLOOR" ]; then
    fh=$(fork_height "$(state_load "$PRE_FORK_STATE"; echo "${SPEC_BEFORE:-$L8_EXPECTED_SPEC}")") \
        || { echo "forked, but no verifiable fork block: the RPC node must keep old runtime state"; exit 1; }
    echo "forked at #$fh"; exit 0
fi
echo "pre-fork"; exit 2
