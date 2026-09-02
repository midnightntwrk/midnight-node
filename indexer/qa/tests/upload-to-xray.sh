#!/bin/bash

# Good to perform some checks and the below vars and report in case of missing
XRAY_BASE_URL="https://eu.xray.cloud.getxray.app/api/v2"
AUTH_URL="${XRAY_BASE_URL}/authenticate"
REPORT_PATH=$XRAY_REPORT_PATH

# Check if XRAY_CLIENT_ID and XRAY_CLIENT_SECRET are defined as environment variables
if [ -z "$XRAY_CLIENT_ID" ]; then
    echo "[ERROR] - XRAY_CLIENT_ID environment variable is not set"
    echo "[INFO] - Please set XRAY_CLIENT_ID environment variable before running this script"
    exit 1
fi

if [ -z "$XRAY_CLIENT_SECRET" ]; then
    echo "[ERROR] - XRAY_CLIENT_SECRET environment variable is not set"
    echo "[INFO] - Please set XRAY_CLIENT_SECRET environment variable before running this script"
    exit 1
fi

echo "[INFO] - Authenticating in Xray..."

AUTH_DATA="{\"client_id\": \"${XRAY_CLIENT_ID}\", \"client_secret\": \"${XRAY_CLIENT_SECRET}\"}"

XRAY_SESSION_TOKEN=$(curl -X POST $AUTH_URL \
  -H "Content-Type: application/json" \
  -d "${AUTH_DATA}" --silent)

if [ -z "$XRAY_SESSION_TOKEN" ]; then
  echo "[ERROR] - Authentication failed. Exiting..."
  exit 1
fi

XRAY_SESSION_TOKEN=${XRAY_SESSION_TOKEN//\"/}

echo "[INFO] - Authentication successful! Token received"


# Also this maybe good to be a parameter or env variable
TEST_RESULT_FILE="./reports/xray/test-results.json"

echo "[INFO] - Uploading test results to XRay..."

# JUnit Format
# RESPONSE=$(curl -X POST "${XRAY_BASE_URL}/import/execution/junit?projectKey=PM&testExecKey=PM-17151" \
#  -H "Authorization: Bearer ${XRAY_SESSION_TOKEN}" \
#  -H "Content-Type: text/xml" \
#  --data @"${TEST_RESULT_FILE}" )

# XRay JSON Format
RESPONSE=$(curl -X POST "${XRAY_BASE_URL}/import/execution" \
 -H "Authorization: Bearer ${XRAY_SESSION_TOKEN}" \
 -H "Content-Type: application/json" \
 --data @"${TEST_RESULT_FILE}" )

echo "XRAY API responded with: ${RESPONSE}"
EXECUTION_ID=$(echo $RESPONSE | grep -o '"key":"[^"]*"' | sed 's/"key":"\([^"]*\)"/\1/')
echo $EXECUTION_ID > xray_id.txt

echo "✅ Upload completed."
