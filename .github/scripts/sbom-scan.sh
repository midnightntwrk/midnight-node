#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Generate SBOM with Syft, scan with Grype, and attest SBOM with Cosign.
#
# Attestation strategy:
#   - Release builds: attest with tlog-upload=true for full supply-chain assurance
#     via the Sigstore transparency log (Rekor).
#   - Non-release builds (CI, main merges): skip attestation entirely
#     (skip-attestation=true in the workflow). SBOM generation and vulnerability
#     scanning still run. This avoids Rekor connectivity failures in cosign v3's
#     bundle signing path, where --tlog-upload=false alone is insufficient.
#
# Usage:
#   source .github/scripts/sbom-scan.sh
#   generate_sbom_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0" "sbom.spdx.json"
#   trim_sbom_for_attestation "sbom.spdx.json" "sbom-attestation.spdx.json"
#   scan_image_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0" "high" "scan-results.json"
#   attest_sbom_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0" "sbom.spdx.json"

# Note: We intentionally don't use `set -euo pipefail` at the top level because
# this script is designed to be sourced. Those settings would affect the caller's
# shell and cause it to exit on any error. Each function handles errors explicitly.

# Build cosign verify-attestation arguments for a given tlog-upload setting.
# When tlog was not uploaded, verification must skip the tlog check.
_build_verify_args() {
  local tlog_upload="$1"
  local -n _out=$2
  _out=(--type spdxjson --certificate-identity-regexp '.*' --certificate-oidc-issuer-regexp '.*')
  if [ "${tlog_upload}" != "true" ]; then
    _out+=(--insecure-ignore-tlog=true)
  fi
}

# Normalize a tlog-upload value to lowercase "true" or "false".
# GitHub Actions passes boolean inputs as the strings "true"/"false", but
# workflow_call boundaries can introduce case variations.
_normalize_bool() {
  local val="${1,,}"  # bash lowercase
  if [ "${val}" = "true" ]; then echo "true"; else echo "false"; fi
}

generate_sbom_with_retry() {
  local IMAGE="$1"
  local OUTPUT_FILE="$2"
  local PLATFORM="${3:-}"
  local MAX_ATTEMPTS=3
  local DELAY=10

  command -v syft >/dev/null 2>&1 || { echo "::error::syft not found"; return 1; }

  local platform_args=()
  if [ -n "$PLATFORM" ]; then
    platform_args=(--platform "$PLATFORM")
    echo "Generating SBOM for ${IMAGE} (platform: ${PLATFORM})"
  else
    echo "Generating SBOM for ${IMAGE}"
  fi

  for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
    if syft "${platform_args[@]}" "${IMAGE}" --select-catalogers '-file' -o spdx-json="${OUTPUT_FILE}"; then
      echo "Successfully generated SBOM for ${IMAGE}"
      return 0
    fi
    if [ $attempt -lt $MAX_ATTEMPTS ]; then
      echo "SBOM generation failed, retrying in ${DELAY}s..."
      sleep $DELAY
      DELAY=$((DELAY * 2))
    fi
  done

  echo "::error::Failed to generate SBOM for ${IMAGE} after $MAX_ATTEMPTS attempts"
  return 1
}

trim_sbom_for_attestation() {
  local INPUT_FILE="$1"
  local OUTPUT_FILE="$2"

  command -v jq >/dev/null 2>&1 || { echo "::error::jq not found"; return 1; }

  # Strip SPDX relationships to reduce size below the 16MB actions/attest-sbom limit.
  # The full SBOM (with relationships) is preserved separately as a build artifact.
  if ! jq -c 'del(.relationships)' "$INPUT_FILE" > "$OUTPUT_FILE"; then
    echo "::error::Failed to trim SBOM"
    return 1
  fi

  local original_size trimmed_size
  original_size=$(wc -c < "$INPUT_FILE")
  trimmed_size=$(wc -c < "$OUTPUT_FILE")
  echo "Trimmed SBOM for attestation: ${original_size} -> ${trimmed_size} bytes (removed relationships)"

  if [ "$trimmed_size" -gt 16777216 ]; then
    echo "::warning::Trimmed SBOM (${trimmed_size} bytes) still exceeds 16MB limit"
  fi
}

scan_image_with_retry() {
  local IMAGE="$1"
  local SEVERITY_CUTOFF="${2:-high}"
  local OUTPUT_FILE="${3:-}"
  local PLATFORM="${4:-}"
  local MAX_ATTEMPTS=3
  local DELAY=10

  command -v grype >/dev/null 2>&1 || { echo "::error::grype not found"; return 1; }

  if [ -n "$PLATFORM" ]; then
    echo "Scanning ${IMAGE} (platform: ${PLATFORM}) for vulnerabilities (fail on ${SEVERITY_CUTOFF}+)"
  else
    echo "Scanning ${IMAGE} for vulnerabilities (fail on ${SEVERITY_CUTOFF}+)"
  fi

  # Build grype command - always show table output, optionally save JSON
  local grype_cmd=(grype "${IMAGE}" --fail-on "${SEVERITY_CUTOFF}")
  if [ -n "$PLATFORM" ]; then
    grype_cmd+=(--platform "${PLATFORM}")
  fi
  if [ -n "$OUTPUT_FILE" ]; then
    # Show table on stdout AND write JSON to file
    grype_cmd+=(--output table --output "json=${OUTPUT_FILE}")
  fi

  for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
    local exit_code=0
    "${grype_cmd[@]}" || exit_code=$?

    if [ $exit_code -eq 0 ]; then
      echo "No vulnerabilities at or above ${SEVERITY_CUTOFF} severity found in ${IMAGE}"
      return 0
    elif [ $exit_code -eq 2 ]; then
      # Exit code 2 means vulnerabilities were found above threshold - display summary before failing
      if [ -n "$OUTPUT_FILE" ] && [ -f "$OUTPUT_FILE" ]; then
        echo "::group::Vulnerability Summary"
        jq -r '.matches[] | "\(.vulnerability.severity): \(.vulnerability.id) in \(.artifact.name)@\(.artifact.version)"' "$OUTPUT_FILE" 2>/dev/null | sort | uniq -c | sort -rn || true
        echo "::endgroup::"
      fi
      echo "::error::Vulnerabilities at or above ${SEVERITY_CUTOFF} severity found in ${IMAGE}"
      return 1
    else
      # Exit code 1 = general error, other codes = transient failures - retry
      if [ $attempt -lt $MAX_ATTEMPTS ]; then
        echo "Scan failed with exit code ${exit_code}, retrying in ${DELAY}s..."
        sleep $DELAY
        DELAY=$((DELAY * 2))
      fi
    fi
  done

  echo "::error::Failed to scan ${IMAGE} after $MAX_ATTEMPTS attempts"
  return 1
}

attest_sbom_with_retry() {
  local IMAGE="$1"
  local SBOM_FILE="$2"
  local TLOG_UPLOAD
  TLOG_UPLOAD="$(_normalize_bool "${3:-true}")"
  local MAX_ATTEMPTS=3
  local DELAY=10

  command -v cosign >/dev/null 2>&1 || { echo "::error::cosign not found"; return 1; }
  command -v jq >/dev/null 2>&1 || { echo "::error::jq not found"; return 1; }

  local BASE_IMAGE="${IMAGE%%:*}"

  local DIGEST_JSON
  if ! DIGEST_JSON=$(docker manifest inspect "${IMAGE}" --verbose 2>&1); then
    echo "::error::Failed to inspect manifest for ${IMAGE}: ${DIGEST_JSON}"
    return 1
  fi

  local DIGEST
  if echo "$DIGEST_JSON" | jq -e 'type == "array"' > /dev/null 2>&1; then
    DIGEST=$(echo "$DIGEST_JSON" | jq -r '.[0].Descriptor.digest')
  else
    DIGEST=$(echo "$DIGEST_JSON" | jq -r '.Descriptor.digest')
  fi

  if [ -z "$DIGEST" ] || [ "$DIGEST" = "null" ]; then
    echo "::error::Failed to extract digest from manifest for ${IMAGE}"
    echo "::error::Manifest JSON: ${DIGEST_JSON}"
    return 1
  fi

  echo "Attesting SBOM for ${IMAGE} (${DIGEST}) (tlog-upload=${TLOG_UPLOAD})"

  for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
    if cosign attest --yes \
      --tlog-upload="${TLOG_UPLOAD}" \
      --predicate "${SBOM_FILE}" \
      --type spdxjson \
      "${BASE_IMAGE}@${DIGEST}"; then
      echo "Successfully attested SBOM for ${IMAGE}"

      echo "Verifying SBOM attestation..."
      local verify_args
      _build_verify_args "${TLOG_UPLOAD}" verify_args
      if cosign verify-attestation "${verify_args[@]}" \
        "${BASE_IMAGE}@${DIGEST}" > /dev/null 2>&1; then
        echo "SBOM attestation verified successfully"
      else
        echo "::warning::SBOM attestation verification failed - attestation may not be retrievable"
      fi

      return 0
    fi
    if [ $attempt -lt $MAX_ATTEMPTS ]; then
      echo "SBOM attestation failed, retrying in ${DELAY}s..."
      sleep $DELAY
      DELAY=$((DELAY * 2))
    fi
  done

  echo "::error::Failed to attest SBOM for ${IMAGE} after $MAX_ATTEMPTS attempts"
  return 1
}

attest_sbom_to_multiarch() {
  local MULTIARCH_IMAGE="$1"
  local SBOM_FILE="$2"
  local TLOG_UPLOAD
  TLOG_UPLOAD="$(_normalize_bool "${3:-true}")"
  local MAX_ATTEMPTS=3
  local DELAY=10

  command -v cosign >/dev/null 2>&1 || { echo "::error::cosign not found"; return 1; }

  local BASE_IMAGE="${MULTIARCH_IMAGE%%:*}"
  local MANIFEST_LIST_DIGEST
  MANIFEST_LIST_DIGEST="sha256:$(docker buildx imagetools inspect --raw "${MULTIARCH_IMAGE}" | sha256sum | awk '{print $1}')"

  echo "Attesting SBOM to multi-arch manifest: ${BASE_IMAGE}@${MANIFEST_LIST_DIGEST} (tlog-upload=${TLOG_UPLOAD})"

  for ((attempt=1; attempt<=MAX_ATTEMPTS; attempt++)); do
    if cosign attest --yes \
      --tlog-upload="${TLOG_UPLOAD}" \
      --predicate "${SBOM_FILE}" \
      --type spdxjson \
      "${BASE_IMAGE}@${MANIFEST_LIST_DIGEST}"; then
      echo "Successfully attested SBOM to multi-arch manifest"

      echo "Verifying multi-arch SBOM attestation..."
      local verify_args
      _build_verify_args "${TLOG_UPLOAD}" verify_args
      if cosign verify-attestation "${verify_args[@]}" \
        "${BASE_IMAGE}@${MANIFEST_LIST_DIGEST}" > /dev/null 2>&1; then
        echo "Multi-arch SBOM attestation verified successfully"
      else
        echo "::warning::Multi-arch SBOM attestation verification failed - attestation may not be retrievable"
      fi

      return 0
    fi
    if [ $attempt -lt $MAX_ATTEMPTS ]; then
      echo "Multi-arch SBOM attestation failed, retrying in ${DELAY}s..."
      sleep $DELAY
      DELAY=$((DELAY * 2))
    fi
  done

  echo "::error::Failed to attest SBOM to multi-arch manifest after $MAX_ATTEMPTS attempts"
  return 1
}
