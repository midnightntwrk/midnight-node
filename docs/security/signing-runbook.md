# Image Signing Operational Runbook

This runbook covers operational procedures for the image signing and SBOM infrastructure.

## Normal Signing Flow

During a normal release, the following occurs automatically:

1. **Build**: Multi-architecture images are built for `linux/amd64` and `linux/arm64`
2. **Push**: Images are pushed to GHCR and Docker Hub
3. **Sign**: Each image digest is signed with Cosign keyless signing
4. **SBOM**: Syft generates an SPDX-JSON SBOM
5. **Scan**: Grype scans for vulnerabilities (fails on critical)
6. **Attest**: The SBOM is attached as a signed attestation

All steps must succeed for the release to complete.

## Monitoring

### GitHub Actions

Monitor the CI/CD pipeline for signing and scanning jobs:

- [Main workflow](https://github.com/midnightntwrk/midnight-node/actions/workflows/main.yml)
- [Release workflow](https://github.com/midnightntwrk/midnight-node/actions/workflows/release-image.yml)

Look for these job names:

- `sign-node-image` / `sign-toolkit-image`
- `sbom-scan-node` / `sbom-scan-toolkit`

### Sigstore Status

Check Sigstore service health for signing issues:

- Status page: https://status.sigstore.dev/
- Fulcio (certificate authority)
- Rekor (transparency log)

## Troubleshooting

### Signing Failures

#### Symptoms

- `sign-image` job fails
- Error: "error signing"
- Error: "OIDC token exchange failed"

#### Investigation

1. Check Sigstore status at https://status.sigstore.dev/
2. Review workflow logs for specific error messages
3. Verify the workflow has `id-token: write` permission

#### Resolution

**Transient failure (Sigstore outage):**

Re-run the failed job. The signing script has built-in retry logic with exponential backoff (3 attempts, 10s/20s delays).

**OIDC token failure:**

```
Error: Error message: Unable to get ACTIONS_ID_TOKEN_REQUEST_URL env variable
```

Ensure the workflow has the required permission:

```yaml
permissions:
  id-token: write
```

**Registry authentication failure:**

Verify registry credentials are configured in repository secrets:

- `MIDNIGHTCI_PACKAGES_WRITE` for GHCR
- `DOCKERHUB_MIDNIGHTNTWRK_USER` and `DOCKERHUB_MIDNIGHTNTWRK_TOKEN` for Docker Hub

### SBOM Generation Failures

#### Symptoms

- `sbom-scan` job fails during "Generate SBOM" step
- Error: "failed to catalog"

#### Investigation

1. Check if the image exists and is accessible
2. Review Syft error output for specific failures
3. Check for timeout issues on large images

#### Resolution

**Image not found:**

Ensure the image was pushed successfully before the SBOM job runs. Check job dependencies in the workflow.

**Timeout:**

Large images may timeout. The script retries 3 times with exponential backoff.

**Unsupported image format:**

Syft supports OCI and Docker image formats. Verify the image is in a supported format.

### Vulnerability Scan Failures

#### Symptoms

- `sbom-scan` job fails during "Scan for vulnerabilities" step
- Output shows vulnerabilities with critical severity

#### Investigation

1. Review the vulnerability summary in the job output
2. Download the `*-scan-results` artifact for full details
3. Check if vulnerabilities are in base images or application code

#### Resolution

**Option 1: Fix the vulnerability**

Update the affected package to a patched version:

```dockerfile
# For apt packages
RUN apt-get update && apt-get install -y package-name=fixed-version

# For npm packages
RUN npm update vulnerable-package

# For Rust packages
# Update Cargo.lock with patched version
```

**Option 2: Temporarily ignore (with justification)**

If no fix is available, add to `.grype.yaml`:

```yaml
ignore:
  # CVE-YYYY-XXXXX: Description of vulnerability
  # Impact assessment: Explain why this is acceptable to ignore
  # Affected component: What package/binary is affected
  # Tracking: https://github.com/upstream/repo/issues/XXX
  # TODO: Remove when upstream releases fix
  - vulnerability: CVE-YYYY-XXXXX
```

**Requirements for ignoring vulnerabilities:**

1. Document the CVE ID and description
2. Assess and document the risk/impact
3. Link to upstream tracking issue
4. Add TODO with removal criteria
5. Create a tracking issue in this repository

### SBOM Attestation Failures

#### Symptoms

- `sbom-scan` job fails during "Attest SBOM" step
- Error: "error attesting"

#### Investigation

1. Verify the image digest is correct
2. Check that signing succeeded (attestation uses the same mechanism)
3. Review Sigstore status

#### Resolution

Same as signing failures - attestation uses Cosign keyless signing under the hood.

**Fork PR:**

Attestation is automatically skipped for fork PRs (they don't have OIDC tokens). This is expected behavior.

## Managing CVE Ignores

### Adding an Ignore

1. Assess the vulnerability: `grype ghcr.io/midnight-ntwrk/midnight-node:TAG --output json | jq '.matches[] | select(.vulnerability.id == "CVE-YYYY-XXXXX")'`
2. Create tracking issue documenting CVE ID, severity, affected component, impact assessment, and upstream fix status
3. Add to `.grype.yaml` with required comments (see "Option 2" above for format)
4. Create PR referencing the tracking issue

### Removing an Ignore

When a fix becomes available: update the package, remove the ignore entry, close the tracking issue, and create a PR with all changes.

## Emergency Procedures

### Critical Vulnerability in Production

1. Assess which versions/deployments are affected
2. Alert operators via appropriate channels
3. Create hotfix release or document mitigation steps
4. Push fixed images through normal pipeline

### Sigstore Outage

Check status at https://status.sigstore.dev/. The signing script has retry logic. If extended, images can be built without signatures and re-signed later.

## Related Documentation

- [Image Signing Overview](image-signing.md) - Architecture and implementation
- [Verification Guide](verification-guide.md) - How to verify signatures
- [Release Checklist](../operations/release-checklist.md) - Release procedures
