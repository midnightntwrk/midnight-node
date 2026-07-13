# Verifying Midnight Container Images

Before deploying a Midnight node, verify that the image you pulled was built by Midnight's official CI/CD pipeline and has not been tampered with.

## Install GitHub CLI

```bash
# macOS
brew install gh

# Linux (Debian/Ubuntu)
sudo apt install gh

# Linux (Fedora)
sudo dnf install gh
```

## Verify an Image

### Using the verification script

```bash
# Verify build provenance
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:TAG

# Verify build provenance + SBOM attestation
./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:TAG
```

### Using gh directly

```bash
gh attestation verify oci://ghcr.io/midnight-ntwrk/midnight-node:TAG \
    --owner midnightntwrk
```

> **Note:** The GitHub org is `midnightntwrk` (no hyphen). Images are published to both `ghcr.io/midnight-ntwrk` (legacy) and `ghcr.io/midnightntwrk` (preferred).

## Attested Images

| Image | Registry |
|-------|----------|
| Node | `ghcr.io/midnight-ntwrk/midnight-node` |
| Node | `ghcr.io/midnightntwrk/midnight-node` |
| Node | `midnightntwrk/midnight-node` (Docker Hub) |
| Toolkit | `ghcr.io/midnight-ntwrk/midnight-node-toolkit` |
| Toolkit | `ghcr.io/midnightntwrk/midnight-node-toolkit` |
| Toolkit | `midnightntwrk/midnight-node-toolkit` (Docker Hub) |

## What Verification Proves

- The image was built in the `midnightntwrk/midnight-node` GitHub repository
- The build ran on GitHub Actions (not a third-party environment)
- The image has not been modified since it was built

## Troubleshooting

| Error | Meaning | Action |
|-------|---------|--------|
| `no attestations found` | Image is unattested or not found | Check the image reference is correct and from an attested repository |
| `verification failed` | Attestation doesn't match image | Verify you are using an official Midnight image |
| `SBOM attestation not found` | No SBOM attached | Older images may predate SBOM support; verify the build provenance only |

## Best Practices

- **Always verify before deploying to production.** Run verification as part of your deployment process.
- **Pin image versions.** Use specific tags (e.g., `:1.2.3`) or digests (`@sha256:...`) rather than `:latest`.
- **Automate verification.** If running Kubernetes, consider an admission controller like [Kyverno](https://kyverno.io/) to enforce verification at deploy time.
