#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -euo pipefail

log=$(mktemp)
trap 'rm -f "$log"' EXIT

for image in "$@"; do
    [[ -n "$image" ]] || continue
    # Match docker run's default pull policy and preserve already-loaded images.
    if docker image inspect "$image" >/dev/null 2>&1; then
        continue
    fi

    for attempt in 1 2 3; do
        if timeout --kill-after=10s 180s docker pull "$image" >"$log" 2>&1; then
            cat "$log"
            break
        else
            status=$?
        fi
        cat "$log" >&2

        if grep -Eiq 'unauthorized|denied|manifest unknown|not found|invalid reference|no matching manifest|x509' "$log"; then
            exit "$status"
        fi
        if [[ "$status" != 124 ]] && ! grep -Eiq '(^|[^a-z])EOF([^a-z]|$)|connection reset|connection refused|connection timed out|i/o timeout|TLS handshake timeout|temporary failure|too many requests|toomanyrequests|(^|[^0-9])(429|500|502|503|504)([^0-9]|$)' "$log"; then
            exit "$status"
        fi
        if [[ "$attempt" == 3 ]]; then
            exit "$status"
        fi
        echo "Transient image pull failure; retrying in $((attempt * 5)) seconds (attempt $((attempt + 1))/3)." >&2
        sleep "$((attempt * 5))"
    done
done
