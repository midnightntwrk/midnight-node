# Container Image Signing and SBOM Generation

Node images published by Midnight are:

1. **Signed** using [Cosign](https://github.com/sigstore/cosign) keyless signing
2. **Accompanied by an SBOM** (Software Bill of Materials) in SPDX-JSON format
3. **Scanned for vulnerabilities** using [Grype](https://github.com/anchore/grype)

## Architecture

### Keyless Signing with OIDC

Cosign's keyless signing eliminates long-lived signing keys. Instead, signing uses OpenID Connect (OIDC) identity from GitHub Actions:

1. Request OIDC token from GitHub
2. Exchange token with Sigstore Fulcio CA for short-lived certificate
3. Sign image digest and upload signature to Rekor transparency log

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

**Severity Threshold:** Builds fail if any `critical` severity vulnerabilities are found.

## Published Images

The following images are signed and include SBOM attestations:

| Image | Registry | Description |
|-------|----------|-------------|
| `midnight-node` | `ghcr.io/midnight-ntwrk/midnight-node` | Midnight blockchain node |
| `midnight-node` | `midnightntwrk/midnight-node` (Docker Hub) | Midnight blockchain node |
| `midnight-node-toolkit` | `ghcr.io/midnight-ntwrk/midnight-node-toolkit` | Transaction generator and testing tools |
| `midnight-node-toolkit` | `midnightntwrk/midnight-node-toolkit` (Docker Hub) | Transaction generator and testing tools |

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

Images must pass these checks before release:

1. **Build**: Image builds successfully for all architectures
2. **Vulnerability Scan**: No critical vulnerabilities detected
3. **Signing**: Image is signed successfully
4. **SBOM Attestation**: SBOM is generated and attested to the image

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
