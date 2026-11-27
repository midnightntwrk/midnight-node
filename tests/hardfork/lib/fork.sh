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

# FORK_DIR: the directory of the fork under test, forks/<name>, the one with its versions.sh.
# Defaults to the only fork there is.
_HF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${FORK_DIR:-}" ]; then
    _forks=("$_HF_DIR"/forks/*/versions.sh)
    [ "${#_forks[@]}" = 1 ] && [ -s "${_forks[0]}" ] \
        || { echo "set FORK_DIR to a fork directory under $_HF_DIR/forks (one with a versions.sh)" >&2; exit 2; }
    FORK_DIR="$(dirname "${_forks[0]}")"
fi
[ -s "$FORK_DIR/versions.sh" ] || { echo "no versions.sh in $FORK_DIR" >&2; exit 2; }
FORK_DIR="$(cd "$FORK_DIR" && pwd)"; export FORK_DIR
