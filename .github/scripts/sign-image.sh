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

  # Collect all digests (single image = 1 digest, multi-arch manifest = multiple)
  local DIGESTS
  if echo "$DIGEST_JSON" | jq -e 'type == "array"' > /dev/null 2>&1; then
    # Multi-arch manifest: get all platform digests
    DIGESTS=$(echo "$DIGEST_JSON" | jq -r '.[].Descriptor.digest')
  else
    # Single image
    DIGESTS=$(echo "$DIGEST_JSON" | jq -r '.Descriptor.digest')
  fi

  # Sign each digest
  for DIGEST in $DIGESTS; do
    echo "Signing ${IMAGE} (${DIGEST})"

    local attempt
    local signed=false
    DELAY=10  # Reset delay for each digest

    for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
      if cosign sign --yes "${BASE_IMAGE}@${DIGEST}"; then
        echo "Successfully signed ${IMAGE} (${DIGEST})"
        signed=true
        break
      fi
      if [ $attempt -lt $MAX_ATTEMPTS ]; then
        echo "Signing failed, retrying in ${DELAY}s..."
        sleep $DELAY
        DELAY=$((DELAY * 2))
      fi
    done

    if [ "$signed" = false ]; then
      echo "::error::Failed to sign ${IMAGE} (${DIGEST}) after $MAX_ATTEMPTS attempts"
      return 1
    fi
  done

  echo "Successfully signed all digests for ${IMAGE}"
  return 0
}
