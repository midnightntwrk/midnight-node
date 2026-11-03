# Container Image Attestation and SBOM Generation

Node images published by Midnight are:

1. **Attested** using [GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations) (build provenance)
2. **Accompanied by an SBOM** (Software Bill of Materials) in SPDX-JSON format
3. **Scanned for vulnerabilities** using [Grype](https://github.com/anchore/grype)

## Architecture

### GitHub Native Attestations

GitHub artifact attestations use Sigstore under the hood but are fully managed by GitHub — no external Fulcio CA or Rekor transparency log dependencies. The attestation flow:

1. GitHub Actions generates an OIDC token identifying the workflow
2. `actions/attest-build-provenance` creates a SLSA build provenance attestation
3. The attestation is stored in GitHub's attestation API, linked to the image digest
4. Consumers verify with `gh attestation verify`, which checks the attestation against GitHub's API

### Keyed Signing (opt-in)

`actions/attest` needs GitHub Enterprise Cloud for private and internal repositories, so the release attestation steps only run in `midnightntwrk/midnight-node`. A repository that can't attest (for example a private fork) can sign its releases with cosign and an AWS KMS key instead. Set these Actions variables on the repository or its organisation:

| Variable | Value |
|----------|-------|
| `SIGNING_KMS_KEY` | KMS key ID, alias or ARN, used as `awskms:///<value>` |
| `SIGNING_AWS_ROLE_ARN` | IAM role assumed through GitHub OIDC. It needs `kms:Sign` and `kms:GetPublicKey` on the key |
| `SIGNING_AWS_REGION` | Region of the key |

When `SIGNING_KMS_KEY` is unset, every signing step is skipped. When it is set, `release-image.yml` signs:

- the release-tagged node and toolkit images, by digest. The signature is pushed to the image's own registry
- each release tarball, uploaded as `<tarball>.sig`
- the combined `SHA256SUMS`, uploaded as `SHA256SUMS.sig`

Signatures are not uploaded to the public Rekor transparency log (`--tlog-upload=false`), so a private repository publishes nothing about its releases. cosign is pinned to v2 because v3 rejects that flag.

To verify, get the public key from the key owner through a channel you already trust. A key fetched from the release being verified proves nothing. Anyone with `kms:GetPublicKey` can export it:

```bash
cosign public-key --key awskms:///<SIGNING_KMS_KEY> > cosign.pub

cosign verify --key cosign.pub --insecure-ignore-tlog=true <image>@<digest>
cosign verify-blob --key cosign.pub --insecure-ignore-tlog=true \
  --signature SHA256SUMS.sig SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
```

### SBOM Generation

SBOMs are generated using [Syft](https://github.com/anchore/syft) in SPDX-JSON format:

1. **Scan**: Syft analyzes the container image layers (file cataloger disabled to reduce size)
2. **Extract**: Package information is extracted from package managers (apt, npm, cargo, etc.)
3. **Generate**: An SPDX-JSON document is created listing all components
4. **Trim**: SPDX relationships are stripped and JSON is minified to fit under the 16MB `actions/attest-sbom` predicate limit (full SBOM preserved as build artifact)
5. **Attest**: The trimmed SBOM is attached to the image as a cosign attestation (`cosign attest`, see `.github/scripts/sbom-scan.sh`)

#### SBOM Size Constraints

GitHub's attestation API enforces a hard 16MB predicate size limit ([actions/attest-sbom#168](https://github.com/actions/attest-sbom/issues/168)). The midnight-node image contains ~2,000 packages, producing SBOMs that exceed this limit. To fit:

- **File cataloger disabled**: `--select-catalogers '-file'` excludes filesystem entries (~22MB to ~19MB)
- **Relationships stripped**: `jq -c 'del(.relationships)'` removes inter-package dependency edges and minifies JSON (~19MB to ~12MB)

The **full unmodified SBOM** is uploaded as a build artifact for detailed analysis. The **attested SBOM** retains all package identifiers, versions, licenses, and checksums — only relationship edges and JSON whitespace are removed.

### Vulnerability Scanning

[Grype](https://github.com/anchore/grype) scans images against multiple vulnerability databases:

- National Vulnerability Database (NVD)
- GitHub Security Advisories
- OS-specific databases (Alpine, Debian, Ubuntu, etc.)
- Language-specific databases (npm, PyPI, RubyGems, Cargo, etc.)

**Severity Threshold:** Builds fail if any `critical` severity vulnerabilities are found.

## Published Images

The following images are attested and include SBOM attestations:

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

Both architecture variants are individually attested, and the manifest list itself is also attested.

## CI/CD Integration

### Workflows

| Workflow | Purpose |
|----------|---------|
| `.github/workflows/sbom-scan-image.yml` | Reusable workflow for SBOM generation, scanning, and attestation |

### Scripts

| Script | Purpose |
|--------|---------|
| `.github/scripts/sbom-scan.sh` | SBOM generation, trimming for attestation, and vulnerability scanning with retry logic |

### Release Gates

Images must pass these checks before release:

1. **Build**: Image builds successfully for all architectures
2. **Vulnerability Scan**: No critical vulnerabilities detected
3. **Attestation**: Build provenance attestation is created
4. **SBOM Attestation**: SBOM is generated and attested to the image

### Fork PR Handling

For pull requests from forks, SBOM attestation is skipped because fork PRs don't have access to the OIDC token required for attestation. The vulnerability scan still runs to provide feedback.

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

- [Verification Guide](verification-guide.md) - How to verify image attestations and SBOMs
- [Signing Runbook](signing-runbook.md) - Operational procedures
