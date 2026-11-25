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

# The hard fork suite.
#
#   run.sh local                      the whole fork on local-env, start to end
#   run.sh local g7 g8                some steps, on the chain that is already up
#   run.sh <network> <stage> [...]    one stage on a deployed network (network/run.sh lists them)
#
# QUIET=1 prints only the summary of each step. FORK_DIR picks the fork when forks/ has
# more than one.
set -e
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_NAME="${1:?usage: run.sh local [step...] | run.sh <network> <stage>}"
shift
if [ "$TARGET_NAME" = local ]; then exec "$HERE/local/run.sh" "$@"; fi
exec "$HERE/network/run.sh" "$TARGET_NAME" "$@"
