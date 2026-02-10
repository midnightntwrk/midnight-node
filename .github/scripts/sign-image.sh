#!/usr/bin/env bash
# Sign a container image with cosign, with retry logic and exponential backoff.
#
# Usage:
#   source .github/scripts/sign-image.sh
#   sign_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0"

set -euo pipefail

# Sign a single image reference with retry and exponential backoff.
_cosign_sign_with_retry() {
  local REF="$1"
  local LABEL="$2"
  local MAX_ATTEMPTS=3
  local DELAY=10

  echo "Signing ${LABEL}"

  local attempt
  for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
    if cosign sign --yes "${REF}"; then
      echo "Successfully signed ${LABEL}"
      return 0
    fi
    if [ $attempt -lt $MAX_ATTEMPTS ]; then
      echo "Signing failed, retrying in ${DELAY}s..."
      sleep $DELAY
      DELAY=$((DELAY * 2))
    fi
  done

  echo "::error::Failed to sign ${LABEL} after $MAX_ATTEMPTS attempts"
  return 1
}

sign_with_retry() {
  local IMAGE="$1"

  # Extract base image (without tag) for signing
  local BASE_IMAGE="${IMAGE%%:*}"

  # Get the digest from the manifest
  local DIGEST_JSON
  DIGEST_JSON=$(docker manifest inspect "${IMAGE}" --verbose)

  # Collect all digests (single image = 1 digest, multi-arch manifest = multiple)
  local DIGESTS
  local IS_MULTIARCH=false
  if echo "$DIGEST_JSON" | jq -e 'type == "array"' > /dev/null 2>&1; then
    # Multi-arch manifest: get all platform digests
    IS_MULTIARCH=true
    DIGESTS=$(echo "$DIGEST_JSON" | jq -r '.[].Descriptor.digest')
  else
    # Single image
    DIGESTS=$(echo "$DIGEST_JSON" | jq -r '.Descriptor.digest')
  fi

  # Sign each platform digest
  for DIGEST in $DIGESTS; do
    _cosign_sign_with_retry "${BASE_IMAGE}@${DIGEST}" "${IMAGE} (${DIGEST})" || return 1
  done

  # For multi-arch manifests, also sign the manifest list index itself.
  # This allows `cosign verify IMAGE:TAG` to work directly with the tag.
  # Use digest reference instead of tag (cosign v3 deprecates tag-based signing).
  if [ "$IS_MULTIARCH" = true ]; then
    local MANIFEST_LIST_DIGEST
    MANIFEST_LIST_DIGEST="sha256:$(docker buildx imagetools inspect --raw "${IMAGE}" | sha256sum | awk '{print $1}')"
    _cosign_sign_with_retry "${BASE_IMAGE}@${MANIFEST_LIST_DIGEST}" "manifest list ${IMAGE} (${MANIFEST_LIST_DIGEST})" || return 1
  fi

  echo "Successfully signed all digests for ${IMAGE}"
  return 0
}
