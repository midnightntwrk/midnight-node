#!/usr/bin/env bash
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
