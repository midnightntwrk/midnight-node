# Container Image Verification

Midnight container images are cryptographically signed using [Sigstore](https://www.sigstore.dev/) keyless signing. This allows operators and SPOs to verify that images were legitimately built by Midnight's CI/CD pipeline.

## Quick Start

Verify an image with a single command:

```bash
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:1.0.0
```

## Prerequisites

Install [cosign](https://docs.sigstore.dev/cosign/system_config/installation/):

```bash
# macOS
brew install cosign

# Linux (download binary)
curl -sSfL https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64 -o cosign
chmod +x cosign
sudo mv cosign /usr/local/bin/

# Go
go install github.com/sigstore/cosign/v2/cmd/cosign@latest
```

## What Gets Verified

### Image Signatures

Every Midnight container image is signed during the CI/CD build process using GitHub Actions' OIDC identity. The signature proves:

- The image was built by the `midnightntwrk/midnight-node` GitHub repository
- The build ran on GitHub Actions infrastructure
- The image contents have not been tampered with since signing

### SBOM Attestations

Images also include signed Software Bill of Materials (SBOM) attestations in SPDX format. These provide:

- Complete list of packages and dependencies in the image
- License information for included software
- Cryptographic proof the SBOM was generated during the official build

## Signed Images

The following images are signed:

| Image | Registry |
|-------|----------|
| `midnight-node` | `ghcr.io/midnight-ntwrk/midnight-node` |
| `midnight-node` | `ghcr.io/midnightntwrk/midnight-node` |
| `midnight-node` | `midnightntwrk/midnight-node` (Docker Hub) |
| `midnight-node-toolkit` | `ghcr.io/midnight-ntwrk/midnight-node-toolkit` |
| `midnight-node-toolkit` | `ghcr.io/midnightntwrk/midnight-node-toolkit` |
| `midnight-node-toolkit` | `midnightntwrk/midnight-node-toolkit` (Docker Hub) |

**Note:** Indexer images are not currently signed.

## Usage Examples

### Basic Signature Verification

```bash
# Verify GHCR image
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:1.0.0

# Verify Docker Hub image
./scripts/verify-image.sh midnightntwrk/midnight-node:1.0.0

# Verify latest main build
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:latest-main
```

### SBOM Verification

```bash
# Verify both signature and SBOM attestation
./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:1.0.0
```

### Scripted Use

```bash
# Quiet mode for CI/CD pipelines (exit code only)
if ./scripts/verify-image.sh --quiet ghcr.io/midnight-ntwrk/midnight-node:1.0.0; then
    echo "Image verified successfully"
else
    echo "Image verification failed"
    exit 1
fi
```

## Manual Verification

For advanced users who want to run cosign directly:

### Verify Signature

```bash
cosign verify \
    --certificate-identity-regexp "https://github.com/midnightntwrk/midnight-node/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    ghcr.io/midnight-ntwrk/midnight-node:1.0.0
```

### Verify SBOM Attestation

```bash
cosign verify-attestation \
    --type spdxjson \
    --certificate-identity-regexp "https://github.com/midnightntwrk/midnight-node/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    ghcr.io/midnight-ntwrk/midnight-node:1.0.0
```

### Download and Inspect SBOM

```bash
# Download the SBOM
cosign download attestation \
    --predicate-type https://spdx.dev/Document \
    ghcr.io/midnight-ntwrk/midnight-node:1.0.0 | jq -r '.payload' | base64 -d | jq '.predicate'
```

## Troubleshooting

### "no matching signatures"

**Cause:** The image doesn't have a signature or the signature can't be found.

**Solutions:**
- Verify the image reference is correct (check tag/digest)
- Ensure the image is from a signed repository (see list above)
- Check if the image predates signature implementation
- Verify network connectivity to the registry

### "certificate identity mismatch"

**Cause:** The image was not built by Midnight's official CI/CD.

**Solutions:**
- Verify you're using an official Midnight image
- Check if the image was built from a fork or unofficial source
- Contact the Midnight team if you believe this is an error

### "OIDC issuer mismatch"

**Cause:** The image was not built on GitHub Actions.

**Solutions:**
- This likely indicates a non-official build
- Use official images from the signed repositories listed above

### "SBOM attestation not found"

**Cause:** The image doesn't have an SBOM attestation attached.

**Solutions:**
- SBOM attestations were added after signatures; older images may not have them
- The image may be from a workflow that doesn't generate SBOMs
- Try verifying just the signature without the `--sbom` flag

## Security Considerations

- **Always verify before deploying:** Especially in production environments
- **Use specific tags or digests:** Avoid `latest` tags in production
- **Automate verification:** Use admission controllers in Kubernetes
- **Monitor for failures:** Alert on verification failures in CI/CD pipelines
