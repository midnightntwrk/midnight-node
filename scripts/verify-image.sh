#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) 2026 Midnight Foundation
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

# Verify Midnight container image signatures and attestations
#
# This script verifies that a container image was legitimately built by
# Midnight's CI/CD pipeline using Sigstore keyless signing.
#
# Usage:
#   ./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:1.0.0
#   ./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:1.0.0
#   ./scripts/verify-image.sh --quiet midnightntwrk/midnight-node:1.0.0

set -euo pipefail

# Certificate identity pattern for Midnight CI workflows
# Note: GitHub org is "midnightntwrk" (no hyphen), while GHCR uses "midnight-ntwrk"
CERT_IDENTITY_REGEXP="https://github.com/midnightntwrk/midnight-node/.github/workflows/.*"
OIDC_ISSUER="https://token.actions.githubusercontent.com"

# Known signed image prefixes
SIGNED_IMAGE_PREFIXES=(
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
Verify Midnight container image signatures and attestations.

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

check_cosign() {
    if ! command -v cosign &> /dev/null; then
        log_error "cosign is not installed"
        log ""
        log "Install cosign from: https://docs.sigstore.dev/cosign/system_config/installation/"
        log ""
        log "Quick install options:"
        log "  brew install cosign                    # macOS"
        log "  go install github.com/sigstore/cosign/v2/cmd/cosign@latest  # Go"
        log "  curl -sSfL https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64 -o cosign && chmod +x cosign  # Linux"
        exit 1
    fi
}

check_image_is_signed() {
    local image="$1"
    local is_known=false

    for prefix in "${SIGNED_IMAGE_PREFIXES[@]}"; do
        if [[ "$image" == "$prefix"* ]]; then
            is_known=true
            break
        fi
    done

    if [[ "$is_known" != "true" ]]; then
        log_warning "Image '$image' is not a known Midnight signed image"
        log_warning "Known signed images:"
        for prefix in "${SIGNED_IMAGE_PREFIXES[@]}"; do
            log "  - ${prefix}:*"
        done
        log ""
        log "Continuing with verification anyway..."
        log ""
    fi
}

verify_signature() {
    local image="$1"

    log "Verifying signature for: $image"
    log ""

    local cosign_output
    local cosign_exit_code=0

    cosign_output=$(cosign verify \
        --certificate-identity-regexp "$CERT_IDENTITY_REGEXP" \
        --certificate-oidc-issuer "$OIDC_ISSUER" \
        "$image" 2>&1) || cosign_exit_code=$?

    if [[ $cosign_exit_code -eq 0 ]]; then
        log_success "Signature verification passed"
        if [[ "$QUIET" != "true" ]]; then
            log ""
            log "Certificate details:"
            echo "$cosign_output" | head -20
        fi
        return 0
    else
        log_error "Signature verification failed"
        log ""

        # Provide helpful error messages
        if echo "$cosign_output" | grep -q "no matching signatures"; then
            log "Possible causes:"
            log "  - Image may not be signed (pre-signing release or third-party image)"
            log "  - Image reference may be incorrect"
            log "  - Registry may be unreachable"
        elif echo "$cosign_output" | grep -q "certificate identity"; then
            log "Certificate identity mismatch:"
            log "  - Image was not built by Midnight's CI/CD pipeline"
            log "  - Expected identity pattern: $CERT_IDENTITY_REGEXP"
        elif echo "$cosign_output" | grep -q "issuer"; then
            log "OIDC issuer mismatch:"
            log "  - Image was not built on GitHub Actions"
            log "  - Expected issuer: $OIDC_ISSUER"
        else
            log "Error output:"
            echo "$cosign_output"
        fi

        return 1
    fi
}

verify_sbom_attestation() {
    local image="$1"

    log ""
    log "Verifying SBOM attestation for: $image"
    log ""

    local cosign_output
    local cosign_exit_code=0

    cosign_output=$(cosign verify-attestation \
        --type spdxjson \
        --certificate-identity-regexp "$CERT_IDENTITY_REGEXP" \
        --certificate-oidc-issuer "$OIDC_ISSUER" \
        "$image" 2>&1) || cosign_exit_code=$?

    if [[ $cosign_exit_code -eq 0 ]]; then
        log_success "SBOM attestation verification passed"
        return 0
    else
        log_error "SBOM attestation verification failed"
        log ""

        if echo "$cosign_output" | grep -q "no matching attestations"; then
            log "Possible causes:"
            log "  - Image may not have an SBOM attestation attached"
            log "  - SBOM generation may have been skipped for this build"
        else
            log "Error output:"
            echo "$cosign_output"
        fi

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
    check_cosign

    # Warn if not a known signed image
    check_image_is_signed "$IMAGE"

    # Verify signature
    if ! verify_signature "$IMAGE"; then
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
