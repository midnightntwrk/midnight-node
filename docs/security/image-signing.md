# Container Image Signing and SBOM Generation

This document describes the container image signing and Software Bill of Materials (SBOM) infrastructure for Midnight Node.

## Overview

All container images published by the Midnight project are:

1. **Signed** using [Cosign](https://github.com/sigstore/cosign) keyless signing
2. **Accompanied by an SBOM** (Software Bill of Materials) in SPDX-JSON format
3. **Scanned for vulnerabilities** using [Grype](https://github.com/anchore/grype)

This enables operators to verify the authenticity and integrity of images before deployment and to understand the software components contained within them.

## Why Image Signing Matters

Container image signing provides:

- **Authenticity**: Verify that images were built by Midnight's CI/CD pipeline
- **Integrity**: Detect tampering or corruption of images
- **Non-repudiation**: Cryptographic proof of origin that cannot be denied
- **Supply chain security**: Protect against compromised registries or man-in-the-middle attacks

## Architecture

### Keyless Signing with OIDC

We use Cosign's keyless signing mode, which eliminates the need to manage long-lived signing keys. Instead, signing is based on OpenID Connect (OIDC) identity:

```
┌─────────────────────────────────────────────────────────────────┐
│                    GitHub Actions Workflow                       │
│                                                                  │
│  1. Request OIDC token from GitHub                              │
│  2. Exchange token with Sigstore Fulcio CA                      │
│  3. Receive short-lived signing certificate                      │
│  4. Sign image digest with certificate                          │
│  5. Upload signature to Rekor transparency log                  │
└─────────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│  GitHub OIDC    │  │  Sigstore       │  │  Rekor          │
│  Provider       │  │  Fulcio CA      │  │  Transparency   │
│                 │  │                 │  │  Log            │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

**Benefits of keyless signing:**

- No private keys to manage, rotate, or protect
- Signing identity tied to GitHub Actions workflow identity
- All signatures are recorded in a public transparency log (Rekor)
- Short-lived certificates (10 minutes) minimize exposure window

### SBOM Generation

SBOMs are generated using [Syft](https://github.com/anchore/syft) in SPDX-JSON format:

1. **Scan**: Syft analyzes the container image layers
2. **Extract**: Package information is extracted from package managers (apt, npm, cargo, etc.)
3. **Generate**: An SPDX-JSON document is created listing all components
4. **Attest**: The SBOM is attached to the image as a signed attestation

### Vulnerability Scanning

[Grype](https://github.com/anchore/grype) scans images against multiple vulnerability databases:

- National Vulnerability Database (NVD)
- GitHub Security Advisories
- OS-specific databases (Alpine, Debian, Ubuntu, etc.)
- Language-specific databases (npm, PyPI, RubyGems, Cargo, etc.)

**Severity Threshold:**

The CI pipeline uses a severity threshold of `high`, meaning builds fail if any vulnerabilities with severity `high` or `critical` are found.

## Published Images

The following images are signed and include SBOM attestations:

| Image | Registry | Description |
|-------|----------|-------------|
| `midnight-node` | `ghcr.io/midnightntwrk/midnight-node` | Midnight blockchain node |
| `midnight-node` | `midnightntwrk/midnight-node` (Docker Hub) | Midnight blockchain node |
| `midnight-toolkit` | `ghcr.io/midnightntwrk/midnight-toolkit` | Transaction generator and testing tools |
| `midnight-toolkit` | `midnightntwrk/midnight-toolkit` (Docker Hub) | Transaction generator and testing tools |

## Multi-Architecture Support

All images are published as multi-architecture manifests supporting:

- `linux/amd64` (x86_64)
- `linux/arm64` (aarch64)

Both architecture variants are individually signed, and the manifest list itself is also signed.

## CI/CD Integration

### Workflows

| Workflow | Purpose |
|----------|---------|
| `.github/workflows/sign-image.yml` | Reusable workflow for image signing |
| `.github/workflows/sbom-scan-image.yml` | Reusable workflow for SBOM generation, scanning, and attestation |

### Scripts

| Script | Purpose |
|--------|---------|
| `.github/scripts/sign-image.sh` | Image signing with retry logic and multi-arch support |
| `.github/scripts/sbom-scan.sh` | SBOM generation, vulnerability scanning, and attestation |

### Release Gates

Images must pass the following checks before release:

1. **Build**: Image builds successfully for all architectures
2. **Vulnerability Scan**: No high or critical vulnerabilities detected
3. **Signing**: Image is signed successfully
4. **SBOM Attestation**: SBOM is generated and attested to the image

If any check fails, the release is blocked.

### Fork PR Handling

For pull requests from forks, SBOM attestation is skipped because fork PRs don't have access to the OIDC token required for keyless signing. The vulnerability scan still runs to provide feedback.

## Vulnerability Ignore Configuration

Known vulnerabilities that cannot be immediately fixed can be temporarily ignored using `.grype.yaml`:

```yaml
ignore:
  # CVE-YYYY-XXXXX: Brief description
  # Justification for ignoring
  # Tracking: link to upstream issue
  # TODO: Remove when fix is available
  - vulnerability: CVE-YYYY-XXXXX
```

Each ignore entry must include:

- Description of the vulnerability
- Justification for ignoring (risk assessment)
- Link to upstream tracking issue
- TODO comment with removal criteria

See [Signing Runbook](signing-runbook.md) for procedures on managing CVE ignores.

## Next Steps

- [Verification Guide](verification-guide.md) - How to verify image signatures and SBOMs
- [Signing Runbook](signing-runbook.md) - Operational procedures for signing
