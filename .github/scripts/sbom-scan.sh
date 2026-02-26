#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) 2026 Midnight Foundation
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

# Generate SBOM with Syft and scan with Grype.
#
# Usage:
#   source .github/scripts/sbom-scan.sh
#   generate_sbom_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0" "sbom.spdx.json"
#   scan_image_with_retry "ghcr.io/midnight-ntwrk/midnight-node:v1.0.0" "high" "scan-results.json"

# Note: We intentionally don't use `set -euo pipefail` at the top level because
# this script is designed to be sourced. Those settings would affect the caller's
# shell and cause it to exit on any error. Each function handles errors explicitly.

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
    if syft "${platform_args[@]}" "${IMAGE}" -o spdx-json="${OUTPUT_FILE}"; then
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
