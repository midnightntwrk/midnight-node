#!/usr/bin/env bash
# Sign a binary file with cosign sign-blob, with retry logic and exponential backoff.
#
# This script provides keyless signing using Sigstore's Fulcio CA and Rekor transparency log.
# It is designed to be sourced by GitHub Actions workflows.
#
# Usage:
#   source .github/scripts/sign-binary.sh
#   sign_blob_with_retry "path/to/file.tar.gz"
#
# Outputs:
#   - ${FILE}.bundle - The Sigstore bundle (signature + certificate + transparency log entry)

set -euo pipefail

sign_blob_with_retry() {
  local FILE="$1"
  local MAX_ATTEMPTS=3
  local DELAY=10

  for attempt in $(seq 1 $MAX_ATTEMPTS); do
    echo "Signing $FILE (attempt $attempt/$MAX_ATTEMPTS)..."

    if cosign sign-blob "$FILE" \
        --yes \
        --bundle "${FILE}.bundle"; then
      echo "Successfully signed $FILE"
      echo "  Bundle: ${FILE}.bundle"
      return 0
    fi

    if [ "$attempt" -lt "$MAX_ATTEMPTS" ]; then
      echo "Signing failed, retrying in ${DELAY}s..."
      sleep "$DELAY"
      DELAY=$((DELAY * 2))
    fi
  done

  echo "::error::Failed to sign $FILE after $MAX_ATTEMPTS attempts"
  return 1
}
