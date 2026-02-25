# Image Signature and SBOM Verification Guide

This guide explains how to verify container image signatures and SBOMs for Midnight images.

**Quick Start:** Use the included verification script for simple verification:
```bash
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:TAG
./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:TAG  # Also verify SBOM
```

For manual verification or advanced use cases, continue reading.

## Prerequisites

Install [Cosign](https://github.com/sigstore/cosign):

```bash
# macOS
brew install cosign

# Linux (via Go)
go install github.com/sigstore/cosign/v2/cmd/cosign@latest

# Or download from releases
# https://github.com/sigstore/cosign/releases
```

For SBOM inspection, you'll also need [jq](https://stedolan.github.io/jq/):

```bash
# macOS
brew install jq

# Ubuntu/Debian
apt-get install jq
```

## Verifying Image Signatures

### Basic Signature Verification

Verify that an image was signed by Midnight's CI/CD pipeline:

```bash
# GHCR
cosign verify ghcr.io/midnight-ntwrk/midnight-node:TAG \
  --certificate-identity-regexp '.*' \
  --certificate-oidc-issuer-regexp '.*'

# Docker Hub
cosign verify midnightntwrk/midnight-node:TAG \
  --certificate-identity-regexp '.*' \
  --certificate-oidc-issuer-regexp '.*'
```

Replace `TAG` with the specific version (e.g., `v1.0.0`, `latest`).

### Strict Verification (Recommended for Production)

For production deployments, verify the exact OIDC issuer and identity:

```bash
cosign verify ghcr.io/midnight-ntwrk/midnight-node:TAG \
  --certificate-identity-regexp 'https://github.com/midnightntwrk/midnight-node/.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com'
```

This ensures the image was signed by a GitHub Actions workflow in the `midnightntwrk/midnight-node` repository.

### Example Output

Successful verification:

```
Verification for ghcr.io/midnight-ntwrk/midnight-node:v1.0.0 --
The following checks were performed on each of these signatures:
  - The cosign claims were validated
  - Existence of the claims in the transparency log was verified offline
  - The code-signing certificate was verified using trusted certificate authority certificates

[{"critical":{"identity":{"docker-reference":"ghcr.io/midnight-ntwrk/midnight-node"},...}]
```

Failed verification (unsigned image):

```
Error: no matching signatures:
no signatures found
```

## Verifying SBOM Attestations

### Basic SBOM Verification

Verify that an SBOM attestation exists and is properly signed:

```bash
cosign verify-attestation --type spdxjson \
  ghcr.io/midnight-ntwrk/midnight-node:TAG \
  --certificate-identity-regexp '.*' \
  --certificate-oidc-issuer-regexp '.*'
```

### Downloading the SBOM

Extract and view the SBOM content:

```bash
# Download and decode the SBOM
cosign download attestation ghcr.io/midnight-ntwrk/midnight-node:TAG | \
  jq -r '.payload' | base64 -d | jq '.predicate' > sbom.spdx.json

# View the SBOM
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

# Verify a specific architecture digest
cosign verify ghcr.io/midnight-ntwrk/midnight-node@sha256:DIGEST \
  --certificate-identity-regexp '.*' \
  --certificate-oidc-issuer-regexp '.*'
```

## All Published Images

Verify any of these images using the commands above:

| Image | GHCR | Docker Hub |
|-------|------|------------|
| Midnight Node | `ghcr.io/midnight-ntwrk/midnight-node` | `midnightntwrk/midnight-node` |
| Midnight Toolkit | `ghcr.io/midnight-ntwrk/midnight-node-toolkit` | `midnightntwrk/midnight-node-toolkit` |

> **Note:** The GitHub org is `midnightntwrk` (no hyphen), while GHCR uses `midnight-ntwrk` (with hyphen). Keep this in mind when constructing image references or certificate identity patterns.

## Troubleshooting

### "no signatures found"

The image may not be signed. This can occur if:

- The image predates the signing implementation
- The image is from a fork PR (signatures are skipped)
- There was a signing failure during the build

### "certificate identity mismatch"

The signing identity doesn't match your expected pattern. Check:

- The image is from the official Midnight repository
- You're using the correct identity pattern

### "transparency log lookup failed"

Network issue connecting to Rekor. Verify:

- Internet connectivity
- Rekor service status: https://status.sigstore.dev/

### Offline Verification

For air-gapped environments, you can download the Rekor bundle for offline verification:

```bash
# Download signature with bundle
cosign download signature ghcr.io/midnight-ntwrk/midnight-node:TAG > sig.json

# Verify offline
cosign verify --offline --bundle sig.json ghcr.io/midnight-ntwrk/midnight-node:TAG
```

## Related Documentation

- [Image Signing Overview](image-signing.md) - Architecture and implementation details
- [Signing Runbook](signing-runbook.md) - Operational procedures
