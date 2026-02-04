#!/usr/bin/env bash
# Sign a container image with cosign, with retry logic and exponential backoff.
#
# Usage:
#   source .github/scripts/sign-image.sh
#   sign_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0"

set -euo pipefail

sign_with_retry() {
  local IMAGE="$1"
  local MAX_ATTEMPTS=3
  local DELAY=10

  # Extract base image (without tag) for signing
  local BASE_IMAGE="${IMAGE%%:*}"

  # Get the digest from the manifest
  local DIGEST_JSON
  DIGEST_JSON=$(docker manifest inspect "${IMAGE}" --verbose)

  local DIGEST
  if echo "$DIGEST_JSON" | jq -e 'type == "array"' > /dev/null 2>&1; then
    DIGEST=$(echo "$DIGEST_JSON" | jq -r '.[0].Descriptor.digest')
  else
    DIGEST=$(echo "$DIGEST_JSON" | jq -r '.Descriptor.digest')
  fi

  echo "Signing ${IMAGE} (${DIGEST})"

  for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
    if cosign sign --yes "${BASE_IMAGE}@${DIGEST}"; then
      echo "Successfully signed ${IMAGE}"
      return 0
    fi
    if [ $attempt -lt $MAX_ATTEMPTS ]; then
      echo "Signing failed, retrying in ${DELAY}s..."
      sleep $DELAY
      DELAY=$((DELAY * 2))
    fi
  done

  echo "::error::Failed to sign ${IMAGE} after $MAX_ATTEMPTS attempts"
  return 1
}
