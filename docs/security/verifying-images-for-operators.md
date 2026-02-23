# Verifying Midnight Container Images

Before deploying a Midnight node, verify that the image you pulled was built by Midnight's official CI/CD pipeline and has not been tampered with.

## Install Cosign

```bash
# macOS
brew install cosign

# Linux
curl -sSfL https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64 -o cosign
chmod +x cosign && sudo mv cosign /usr/local/bin/
```

## Verify an Image

### Using the verification script

```bash
# Verify signature
./scripts/verify-image.sh ghcr.io/midnight-ntwrk/midnight-node:TAG

# Verify signature + SBOM attestation
./scripts/verify-image.sh --sbom ghcr.io/midnight-ntwrk/midnight-node:TAG
```

### Using cosign directly

```bash
cosign verify \
  --certificate-identity-regexp 'https://github.com/midnightntwrk/midnight-node/.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  ghcr.io/midnight-ntwrk/midnight-node:TAG
```

> **Note:** The GitHub org is `midnightntwrk` (no hyphen). Images are published to both `ghcr.io/midnight-ntwrk` (legacy) and `ghcr.io/midnightntwrk` (preferred).

## Signed Images

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
- The image has not been modified since it was signed

## Troubleshooting

| Error | Meaning | Action |
|-------|---------|--------|
| `no matching signatures` | Image is unsigned or not found | Check the image reference is correct and from a signed repository |
| `certificate identity mismatch` | Image was not built by Midnight CI | Verify you are using an official Midnight image |
| `OIDC issuer mismatch` | Image was not built on GitHub Actions | Use official images from the repositories listed above |
| `SBOM attestation not found` | No SBOM attached | Older images may predate SBOM support; verify the signature only |

## Best Practices

- **Always verify before deploying to production.** Run verification as part of your deployment process.
- **Pin image versions.** Use specific tags (e.g., `:1.2.3`) or digests (`@sha256:...`) rather than `:latest`.
- **Automate verification.** If running Kubernetes, consider an admission controller like [Kyverno](https://kyverno.io/) or [Sigstore Policy Controller](https://docs.sigstore.dev/policy-controller/overview/) to enforce verification at deploy time.
