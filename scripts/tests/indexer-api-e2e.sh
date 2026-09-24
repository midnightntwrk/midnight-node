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

set -euo pipefail

READY_URL="http://localhost:8088/ready"
TIMEOUT_SECS=60

echo "📡 Polling Indexer readiness at $READY_URL (timeout: ${TIMEOUT_SECS}s)..."

elapsed=0
while [ "$elapsed" -lt "$TIMEOUT_SECS" ]; do
    HTTP_CODE=$(curl -s -o /tmp/ready_response.txt -w "%{http_code}" --max-time 2 "$READY_URL" 2>/dev/null || echo "000")
    BODY=$(cat /tmp/ready_response.txt 2>/dev/null || echo "")

    if [[ "$HTTP_CODE" == "200" && -z "$BODY" ]]; then
        echo "✅ Indexer is ready (200 + empty body) after ${elapsed}s"
        exit 0
    fi

    sleep 1
    elapsed=$((elapsed + 1))
done

echo "❌ Indexer not ready after ${TIMEOUT_SECS}s (last HTTP $HTTP_CODE, body: $BODY)"
exit 1
