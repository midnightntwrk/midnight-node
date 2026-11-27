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

# Group 5: ledger state migration. Only when the fork migrates the ledger state.
#
# Did the migration change exactly what its design says, and can users recover what was
# reset? The whole group is the fork's (forks/<fork>/<this file>); without that file the
# group is one skipped row.
#
# It runs before group 6, so a migration that resets what pays fees can restore it first.
set -e
source "$(dirname "${BASH_SOURCE[0]}")/../lib/suite.sh"

start_after_fork g5

group_start G5 "Ledger state migration of the ledger $OLD_LEDGER -> $NEW_LEDGER fork, on $NETWORK_NAME (fork block #$FORK_HEIGHT)"

if fork_has_checks; then
    run_fork_checks
else
    bullet "Only when the fork migrates the ledger state"
    check no-migration "the state migration" skip "this fork migrates no ledger state"
fi

group_finish
