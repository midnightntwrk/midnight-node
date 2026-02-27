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

# Verify Midnight binary release attestations using GitHub artifact attestations.
#
# This script verifies that a binary release was legitimately built by
# Midnight's CI/CD pipeline using GitHub's native attestation system.
#
# Usage:
#   ./scripts/verify-binary.sh midnight-node-node-1.0.0-linux-amd64.tar.gz
#   ./scripts/verify-binary.sh --checksum midnight-node-node-1.0.0-linux-amd64.tar.gz
#   ./scripts/verify-binary.sh --quiet midnight-node-node-1.0.0-linux-amd64.tar.gz

set -euo pipefail

REPO="midnightntwrk/midnight-node"

# Colors for output (disabled in quiet mode)
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Verify Midnight binary release attestations.

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

Required:
    BINARY_FILE   - The binary file downloaded from a GitHub release

Optional files:
    SHA256SUMS    - Checksums file (required if --checksum is used)

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

verify_provenance() {
    local binary="$1"

    log "Verifying build provenance for: $binary"
    log ""

    local gh_output
    local gh_exit_code=0

    gh_output=$(gh attestation verify "$binary" \
        --repo "$REPO" 2>&1) || gh_exit_code=$?

    if [[ $gh_exit_code -eq 0 ]]; then
        log_success "Build provenance verification passed"
        if [[ "$QUIET" != "true" ]]; then
            log ""
            log "Attestation details:"
            echo "$gh_output" | head -20
        fi
        log ""
        log "This binary was built by the official Midnight Network CI/CD pipeline."
        return 0
    else
        log_error "Build provenance verification failed"
        log ""
        log "Error output:"
        echo "$gh_output"
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
    check_gh

    # Check binary file exists
    if [[ ! -f "$BINARY" ]]; then
        log_error "Binary file not found: $BINARY"
        exit 1
    fi

    # Verify build provenance
    if ! verify_provenance "$BINARY"; then
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
