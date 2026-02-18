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

# Verify Midnight binary release signatures
#
# This script verifies that a binary release was legitimately built by
# Midnight's CI/CD pipeline using Sigstore keyless signing.
#
# Usage:
#   ./scripts/verify-binary.sh midnight-node-node-1.0.0-linux-amd64.tar.gz
#   ./scripts/verify-binary.sh --checksum midnight-node-node-1.0.0-linux-amd64.tar.gz
#   ./scripts/verify-binary.sh --quiet midnight-node-node-1.0.0-linux-amd64.tar.gz

set -euo pipefail

# Certificate identity pattern for Midnight CI workflows
CERT_IDENTITY_REGEXP="https://github.com/midnightntwrk/midnight-node/.github/workflows/.*"
OIDC_ISSUER="https://token.actions.githubusercontent.com"

# Colors for output (disabled in quiet mode)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Verify Midnight binary release signatures.

Usage:
    $(basename "$0") [OPTIONS] BINARY_FILE

Options:
    --checksum  Also verify SHA256 checksum against SHA256SUMS file
    --quiet     Suppress output, use exit codes only (0=success, 1=failure)
    --help      Show this help message

Examples:
    $(basename "$0") midnight-node-node-1.0.0-linux-amd64.tar.gz
    $(basename "$0") --checksum midnight-node-node-1.0.0-linux-amd64.tar.gz
    $(basename "$0") --quiet midnight-node-toolkit-node-1.0.0-linux-arm64.tar.gz

Required files (must be in same directory as BINARY_FILE):
    BINARY_FILE.sig   - Detached signature file
    BINARY_FILE.pem   - Certificate file

Optional files:
    SHA256SUMS        - Checksums file (required if --checksum is used)

Exit codes:
    0   Verification successful
    1   Verification failed or error occurred

For more information, see docs/security/binary-verification.md
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

check_required_files() {
    local binary="$1"
    local sig_file="${binary}.sig"
    local cert_file="${binary}.pem"

    if [[ ! -f "$binary" ]]; then
        log_error "Binary file not found: $binary"
        exit 1
    fi

    if [[ ! -f "$sig_file" ]]; then
        log_error "Signature file not found: $sig_file"
        log ""
        log "Download it from the GitHub release along with the binary."
        exit 1
    fi

    if [[ ! -f "$cert_file" ]]; then
        log_error "Certificate file not found: $cert_file"
        log ""
        log "Download it from the GitHub release along with the binary."
        exit 1
    fi
}

verify_signature() {
    local binary="$1"
    local sig_file="${binary}.sig"
    local cert_file="${binary}.pem"

    log "Verifying signature for: $binary"
    log "  Signature:   $sig_file"
    log "  Certificate: $cert_file"
    log ""

    local cosign_output
    local cosign_exit_code=0

    cosign_output=$(cosign verify-blob \
        --certificate "$cert_file" \
        --signature "$sig_file" \
        --certificate-identity-regexp "$CERT_IDENTITY_REGEXP" \
        --certificate-oidc-issuer "$OIDC_ISSUER" \
        "$binary" 2>&1) || cosign_exit_code=$?

    if [[ $cosign_exit_code -eq 0 ]]; then
        log_success "Signature verification passed"
        log ""
        log "This binary was signed by the official Midnight Network CI/CD pipeline."
        return 0
    else
        log_error "Signature verification failed"
        log ""

        # Provide helpful error messages
        if echo "$cosign_output" | grep -q "certificate identity"; then
            log "Certificate identity mismatch:"
            log "  - Binary was not built by Midnight's CI/CD pipeline"
            log "  - Expected identity pattern: $CERT_IDENTITY_REGEXP"
        elif echo "$cosign_output" | grep -q "issuer"; then
            log "OIDC issuer mismatch:"
            log "  - Binary was not built on GitHub Actions"
            log "  - Expected issuer: $OIDC_ISSUER"
        elif echo "$cosign_output" | grep -q "signature"; then
            log "Signature validation failed:"
            log "  - Binary contents may have been modified"
            log "  - Signature file may be corrupted or for a different file"
        else
            log "Error output:"
            echo "$cosign_output"
        fi

        log ""
        log "WARNING: This binary may not be authentic!"
        return 1
    fi
}

verify_checksum() {
    local binary="$1"
    local binary_dir
    binary_dir=$(dirname "$binary")
    local binary_name
    binary_name=$(basename "$binary")
    local checksums_file="${binary_dir}/SHA256SUMS"

    if [[ ! -f "$checksums_file" ]]; then
        log_error "Checksums file not found: $checksums_file"
        log ""
        log "Download SHA256SUMS from the GitHub release."
        return 1
    fi

    log ""
    log "Verifying checksum for: $binary_name"

    # Extract expected checksum from SHA256SUMS
    local expected_checksum
    expected_checksum=$(grep -E "^[a-f0-9]+\s+${binary_name}$" "$checksums_file" | awk '{print $1}' || true)

    if [[ -z "$expected_checksum" ]]; then
        log_error "No checksum found for $binary_name in $checksums_file"
        return 1
    fi

    # Calculate actual checksum
    local actual_checksum
    if command -v sha256sum &> /dev/null; then
        actual_checksum=$(sha256sum "$binary" | awk '{print $1}')
    elif command -v shasum &> /dev/null; then
        actual_checksum=$(shasum -a 256 "$binary" | awk '{print $1}')
    else
        log_error "Neither sha256sum nor shasum is available"
        return 1
    fi

    if [[ "$expected_checksum" == "$actual_checksum" ]]; then
        log_success "Checksum verification passed"
        log "  Expected: $expected_checksum"
        log "  Actual:   $actual_checksum"
        return 0
    else
        log_error "Checksum verification failed"
        log "  Expected: $expected_checksum"
        log "  Actual:   $actual_checksum"
        return 1
    fi
}

main() {
    local VERIFY_CHECKSUM=false
    local QUIET=false
    local BINARY=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --checksum)
                VERIFY_CHECKSUM=true
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
                if [[ -z "$BINARY" ]]; then
                    BINARY="$1"
                else
                    echo "Error: Multiple files specified" >&2
                    usage
                    exit 1
                fi
                shift
                ;;
        esac
    done

    if [[ -z "$BINARY" ]]; then
        echo "Error: No binary file specified" >&2
        usage
        exit 1
    fi

    # Export QUIET for use in functions
    export QUIET

    # Check prerequisites
    check_cosign

    # Check required files exist
    check_required_files "$BINARY"

    # Verify signature
    if ! verify_signature "$BINARY"; then
        exit 1
    fi

    # Optionally verify checksum
    if [[ "$VERIFY_CHECKSUM" == "true" ]]; then
        if ! verify_checksum "$BINARY"; then
            exit 1
        fi
    fi

    log ""
    log_success "All verifications passed for: $BINARY"
}

main "$@"
