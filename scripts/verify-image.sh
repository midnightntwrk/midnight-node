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

# Verify Midnight container image attestations using GitHub artifact attestations.
#
# This script verifies that a container image was legitimately built by
# Midnight's CI/CD pipeline using GitHub's native attestation system.
#
# Usage:
#   ./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:1.0.0
#   ./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:1.0.0
#   ./scripts/verify-image.sh --quiet midnightntwrk/midnight-node:1.0.0

set -euo pipefail

OWNER="midnightntwrk"

# Known attested image prefixes
ATTESTED_IMAGE_PREFIXES=(
    "ghcr.io/midnight-ntwrk/midnight-node"
    "ghcr.io/midnight-ntwrk/midnight-node-toolkit"
    "ghcr.io/midnightntwrk/midnight-node"
    "ghcr.io/midnightntwrk/midnight-node-toolkit"
    "midnightntwrk/midnight-node"
    "midnightntwrk/midnight-node-toolkit"
)

# Colors for output (disabled in quiet mode)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Verify Midnight container image attestations.

Usage:
    $(basename "$0") [OPTIONS] IMAGE

Options:
    --sbom      Also verify SBOM attestation
    --quiet     Suppress output, use exit codes only (0=success, 1=failure)
    --help      Show this help message

Examples:
    $(basename "$0") ghcr.io/midnight-ntwrk/midnight-node:1.0.0
    $(basename "$0") --sbom ghcr.io/midnight-ntwrk/midnight-node:latest-main
    $(basename "$0") --quiet midnightntwrk/midnight-node:1.0.0

Exit codes:
    0   Verification successful
    1   Verification failed or error occurred

For more information, see docs/security/container-verification.md
EOF
}

log() {
    if [[ "$QUIET" != "true" ]]; then
        echo -e "$@"
    fi
}

log_success() {
    log "${GREEN}✓${NC} $1"
}

log_error() {
    log "${RED}✗${NC} $1"
}

log_warning() {
    log "${YELLOW}!${NC} $1"
}

check_gh() {
    if ! command -v gh &> /dev/null; then
        log_error "gh CLI is not installed"
        log ""
        log "Install the GitHub CLI from: https://cli.github.com/"
        log ""
        log "Quick install options:"
        log "  brew install gh                        # macOS"
        log "  sudo apt install gh                    # Debian/Ubuntu"
        log "  sudo dnf install gh                    # Fedora"
        exit 1
    fi
}

check_image_is_attested() {
    local image="$1"
    local is_known=false

    for prefix in "${ATTESTED_IMAGE_PREFIXES[@]}"; do
        if [[ "$image" == "$prefix"* ]]; then
            is_known=true
            break
        fi
    done

    if [[ "$is_known" != "true" ]]; then
        log_warning "Image '$image' is not a known Midnight attested image"
        log_warning "Known attested images:"
        for prefix in "${ATTESTED_IMAGE_PREFIXES[@]}"; do
            log "  - ${prefix}:*"
        done
        log ""
        log "Continuing with verification anyway..."
        log ""
    fi
}

verify_provenance() {
    local image="$1"

    log "Verifying build provenance for: $image"
    log ""

    local gh_output
    local gh_exit_code=0

    gh_output=$(gh attestation verify "oci://${image}" \
        --owner "$OWNER" 2>&1) || gh_exit_code=$?

    if [[ $gh_exit_code -eq 0 ]]; then
        log_success "Build provenance verification passed"
        if [[ "$QUIET" != "true" ]]; then
            log ""
            log "Attestation details:"
            echo "$gh_output" | head -20
        fi
        return 0
    else
        log_error "Build provenance verification failed"
        log ""
        log "Error output:"
        echo "$gh_output"
        return 1
    fi
}

verify_sbom_attestation() {
    local image="$1"

    log ""
    log "Verifying SBOM attestation for: $image"
    log ""

    local gh_output
    local gh_exit_code=0

    gh_output=$(gh attestation verify "oci://${image}" \
        --owner "$OWNER" \
        --predicate-type https://spdx.dev/Document 2>&1) || gh_exit_code=$?

    if [[ $gh_exit_code -eq 0 ]]; then
        log_success "SBOM attestation verification passed"
        return 0
    else
        log_error "SBOM attestation verification failed"
        log ""
        log "Error output:"
        echo "$gh_output"
        return 1
    fi
}

main() {
    local VERIFY_SBOM=false
    local QUIET=false
    local IMAGE=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --sbom)
                VERIFY_SBOM=true
                shift
                ;;
            --quiet|-q)
                QUIET=true
                shift
                ;;
            --help|-h)
                usage
                exit 0
                ;;
            -*)
                echo "Unknown option: $1" >&2
                usage
                exit 1
                ;;
            *)
                if [[ -z "$IMAGE" ]]; then
                    IMAGE="$1"
                else
                    echo "Error: Multiple images specified" >&2
                    usage
                    exit 1
                fi
                shift
                ;;
        esac
    done

    if [[ -z "$IMAGE" ]]; then
        echo "Error: No image specified" >&2
        usage
        exit 1
    fi

    # Export QUIET for use in functions
    export QUIET

    # Check prerequisites
    check_gh

    # Warn if not a known attested image
    check_image_is_attested "$IMAGE"

    # Verify build provenance
    if ! verify_provenance "$IMAGE"; then
        exit 1
    fi

    # Optionally verify SBOM attestation
    if [[ "$VERIFY_SBOM" == "true" ]]; then
        if ! verify_sbom_attestation "$IMAGE"; then
            exit 1
        fi
    fi

    log ""
    log_success "All verifications passed for: $IMAGE"
}

main "$@"
