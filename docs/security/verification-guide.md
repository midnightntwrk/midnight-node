# Image Attestation and SBOM Verification Guide

This guide explains how to verify container image attestations and SBOMs for Midnight images.

**Quick Start:** Use the included verification script for simple verification:
```bash
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:TAG
./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:TAG  # Also verify SBOM
```

For manual verification or advanced use cases, continue reading.

## Prerequisites

Install the [GitHub CLI](https://cli.github.com/):

```bash
# macOS
brew install gh

# Linux (Debian/Ubuntu)
sudo apt install gh

# Linux (Fedora)
sudo dnf install gh
```

For SBOM inspection, you'll also need [jq](https://stedolan.github.io/jq/):

```bash
# macOS
brew install jq

# Ubuntu/Debian
apt-get install jq
```

## Verifying Image Attestations

### Basic Attestation Verification

Verify that an image was built by Midnight's CI/CD pipeline:

```bash
# GHCR
gh attestation verify oci://ghcr.io/midnight-ntwrk/midnight-node:TAG \
    --owner midnightntwrk

# Docker Hub
gh attestation verify oci://midnightntwrk/midnight-node:TAG \
    --owner midnightntwrk
```

Replace `TAG` with the specific version (e.g., `1.0.0`, `latest-main`).

### Example Output

Successful verification:

```
Loaded digest sha256:abc123... for oci://ghcr.io/midnight-ntwrk/midnight-node:1.0.0
Loaded 1 attestation from GitHub API
✓ Verification succeeded!

sha256:abc123... was attested by a trusted GitHub Actions workflow
```

Failed verification (unattested image):

```
✗ Loading attestations from GitHub API failed

no attestations found for subject
```

## Verifying SBOM Attestations

### Basic SBOM Verification

Verify that an SBOM attestation exists:

```bash
gh attestation verify oci://ghcr.io/midnight-ntwrk/midnight-node:TAG \
    --owner midnightntwrk \
    --predicate-type https://spdx.dev/Document
```

### Downloading the SBOM

Extract and view the SBOM content:

```bash
# Download the SBOM attestation
gh attestation download oci://ghcr.io/midnight-ntwrk/midnight-node:TAG \
    --owner midnightntwrk \
    --predicate-type https://spdx.dev/Document \
    -d /tmp/sbom

# Decode and view the SBOM
cat /tmp/sbom/*.jsonl | jq '.dsseEnvelope.payload' | base64 -d | jq '.predicate' > sbom.spdx.json
cat sbom.spdx.json
```

### Inspecting SBOM Contents

List all packages in the SBOM:

```bash
# List package names and versions
cat sbom.spdx.json | jq -r '.packages[] | "\(.name) \(.versionInfo)"'

# Count packages by type
cat sbom.spdx.json | jq -r '.packages[].externalRefs[]? | select(.referenceCategory == "PACKAGE-MANAGER") | .referenceType' | sort | uniq -c
```

## Verifying Specific Architectures

For multi-architecture images, you can verify specific platform variants:

```bash
# Get the manifest list
docker manifest inspect ghcr.io/midnight-ntwrk/midnight-node:TAG

# Verify a specific architecture by digest
gh attestation verify oci://ghcr.io/midnight-ntwrk/midnight-node@sha256:DIGEST \
    --owner midnightntwrk
```

## All Published Images

Verify any of these images using the commands above:

| Image | GHCR | Docker Hub |
|-------|------|------------|
| Midnight Node | `ghcr.io/midnight-ntwrk/midnight-node` | `midnightntwrk/midnight-node` |
| Midnight Toolkit | `ghcr.io/midnight-ntwrk/midnight-node-toolkit` | `midnightntwrk/midnight-node-toolkit` |

> **Note:** The GitHub org is `midnightntwrk` (no hyphen), while GHCR uses `midnight-ntwrk` (with hyphen). Keep this in mind when constructing image references.

## Troubleshooting

### "no attestations found"

The image may not be attested. This can occur if:

- The image predates the attestation implementation
- The image is from a fork PR (attestations are skipped)
- There was an attestation failure during the build

### "verification failed"

The attestation doesn't match the image. Check:

- The image is from the official Midnight repository
- You're using the correct `--owner` value (`midnightntwrk`)

### Network or API errors

If you get errors connecting to GitHub's attestation API:

- Check internet connectivity
- Check GitHub status: https://www.githubstatus.com/
- Ensure `gh` is authenticated: `gh auth status`

## Related Documentation

- [Image Signing Overview](image-signing.md) - Architecture and implementation details
- [Signing Runbook](signing-runbook.md) - Operational procedures
