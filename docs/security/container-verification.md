# Container Image Verification

Midnight container images are attested using [GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations). This allows operators and SPOs to verify that images were legitimately built by Midnight's CI/CD pipeline.

## Quick Start

Verify an image with a single command:

```bash
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:1.0.0
```

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

## What Gets Verified

### Build Provenance

Every Midnight container image has a build provenance attestation created during CI/CD using `actions/attest-build-provenance`. The attestation proves:

- The image was built by the `midnightntwrk/midnight-node` GitHub repository
- The build ran on GitHub Actions infrastructure
- The image contents have not been tampered with since building

### SBOM Attestations

Images also include SBOM (Software Bill of Materials) attestations in SPDX format, created using `actions/attest-sbom`. These provide:

- Complete list of packages and dependencies in the image
- License information for included software
- Cryptographic proof the SBOM was generated during the official build

## Attested Images

The following images have attestations:

| Image | Registry |
|-------|----------|
| `midnight-node` | `ghcr.io/midnight-ntwrk/midnight-node` |
| `midnight-node` | `ghcr.io/midnightntwrk/midnight-node` |
| `midnight-node` | `midnightntwrk/midnight-node` (Docker Hub) |
| `midnight-node-toolkit` | `ghcr.io/midnight-ntwrk/midnight-node-toolkit` |
| `midnight-node-toolkit` | `ghcr.io/midnightntwrk/midnight-node-toolkit` |
| `midnight-node-toolkit` | `midnightntwrk/midnight-node-toolkit` (Docker Hub) |

**Note:** Indexer images are not currently attested.

## Usage Examples

### Basic Attestation Verification

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
# Verify both build provenance and SBOM attestation
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

For advanced users who want to run `gh` directly:

### Verify Build Provenance

```bash
gh attestation verify oci://ghcr.io/midnight-ntwrk/midnight-node:1.0.0 \
    --owner midnightntwrk
```

### Verify SBOM Attestation

```bash
gh attestation verify oci://ghcr.io/midnight-ntwrk/midnight-node:1.0.0 \
    --owner midnightntwrk \
    --predicate-type https://spdx.dev/Document
```

### Download and Inspect SBOM

```bash
# Download the SBOM attestation
gh attestation download oci://ghcr.io/midnight-ntwrk/midnight-node:1.0.0 \
    --owner midnightntwrk \
    --predicate-type https://spdx.dev/Document \
    -d /tmp/sbom

# Inspect the SBOM content
cat /tmp/sbom/*.jsonl | jq '.dsseEnvelope.payload' | base64 -d | jq '.predicate'
```

## Troubleshooting

### "no attestations found"

**Cause:** The image doesn't have an attestation or the attestation can't be found.

**Solutions:**
- Verify the image reference is correct (check tag/digest)
- Ensure the image is from an attested repository (see list above)
- Check if the image predates attestation implementation
- Verify network connectivity to GitHub

### "SBOM attestation not found"

**Cause:** The image doesn't have an SBOM attestation attached.

**Solutions:**
- SBOM attestations were re-enabled after the migration to GitHub native attestations; older images may not have them
- The image may be from a workflow that doesn't generate SBOMs
- Try verifying just the build provenance without the `--sbom` flag

## Security Considerations

- **Always verify before deploying:** Especially in production environments
- **Use specific tags or digests:** Avoid `latest` tags in production
- **Automate verification:** Use admission controllers in Kubernetes
- **Monitor for failures:** Alert on verification failures in CI/CD pipelines
