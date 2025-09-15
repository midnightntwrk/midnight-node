#!/usr/bin/env bash
set -euo pipefail

READY_URL="http://localhost:8088/ready"

echo "📡 Checking Indexer readiness at $READY_URL..."
echo "Wait 30 seconds to avoid any race conditions..."
sleep 30

HTTP_CODE=$(curl -s -o /tmp/ready_response.txt -w "%{http_code}" "$READY_URL")
BODY=$(cat /tmp/ready_response.txt)


if [[ "$HTTP_CODE" == "200" && -z "$BODY" ]]; then
  echo "✅ Indexer is ready (200 + empty body)"
elif [[ "$HTTP_CODE" == "503" ]]; then
  echo "❌ Indexer not ready (503)"
  exit 1
else
  echo "❌ Unexpected response"
  echo "HTTP $HTTP_CODE"
  echo "Body: $BODY"
  exit 1
fi
