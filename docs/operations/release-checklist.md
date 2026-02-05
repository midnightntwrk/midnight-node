# Release Checklist

This checklist covers the release process for Midnight Node, including security verification steps.

## Pre-Release

### Code Review

- [ ] All PRs merged to release branch
- [ ] CI passing on release branch
- [ ] Runtime metadata regenerated if needed (`/bot rebuild-metadata`)
- [ ] Chain specifications updated if needed (`/bot rebuild-chainspec <network>`)

### Security Verification

- [ ] Review `.grype.yaml` for any temporary CVE ignores
  - Are ignores still necessary?
  - Have upstream fixes been released?
- [ ] Check for new critical CVEs in dependencies
  - Run local scan: `grype ghcr.io/midnight-ntwrk/midnight-node:TAG`
- [ ] Verify no secrets in code or configuration

### Documentation

- [ ] CHANGELOG updated
- [ ] Breaking changes documented
- [ ] Migration guide updated if needed

## Release Build

### Build Verification

- [ ] Images build successfully for all architectures (amd64, arm64)
- [ ] Node image: `ghcr.io/midnight-ntwrk/midnight-node:TAG`
- [ ] Toolkit image: `ghcr.io/midnight-ntwrk/midnight-node-toolkit:TAG`

### Security Pipeline

The following are automated in CI but should be verified:

- [ ] **Image Signing**: Both images signed with Cosign keyless signing
  - Check `sign-node-image` and `sign-toolkit-image` jobs
- [ ] **SBOM Generation**: SBOMs generated for both images
  - Check `sbom-scan-node` and `sbom-scan-toolkit` jobs
- [ ] **Vulnerability Scan**: No critical vulnerabilities
  - Review scan results in job output
  - Download `*-scan-results` artifacts if needed
- [ ] **SBOM Attestation**: SBOMs attested to images
  - Verify with `cosign verify-attestation`

## Post-Release Verification

### Signature Verification

Verify signatures are accessible for all published images:

```bash
# Node - GHCR
cosign verify ghcr.io/midnight-ntwrk/midnight-node:TAG \
  --certificate-identity-regexp 'https://github.com/midnightntwrk/midnight-node/.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com'

# Node - Docker Hub
cosign verify midnightntwrk/midnight-node:TAG \
  --certificate-identity-regexp 'https://github.com/midnightntwrk/midnight-node/.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com'

# Toolkit - GHCR
cosign verify ghcr.io/midnight-ntwrk/midnight-node-toolkit:TAG \
  --certificate-identity-regexp 'https://github.com/midnightntwrk/midnight-node/.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com'

# Toolkit - Docker Hub
cosign verify midnightntwrk/midnight-node-toolkit:TAG \
  --certificate-identity-regexp 'https://github.com/midnightntwrk/midnight-node/.*' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com'
```

### SBOM Verification

Verify SBOM attestations are accessible:

```bash
# Node
cosign verify-attestation --type spdxjson \
  ghcr.io/midnight-ntwrk/midnight-node:TAG \
  --certificate-identity-regexp '.*' \
  --certificate-oidc-issuer-regexp '.*'

# Toolkit
cosign verify-attestation --type spdxjson \
  ghcr.io/midnight-ntwrk/midnight-node-toolkit:TAG \
  --certificate-identity-regexp '.*' \
  --certificate-oidc-issuer-regexp '.*'
```

### Artifact Locations

| Artifact | Location |
|----------|----------|
| Node SBOM | GitHub Actions artifact: `sbom-node-TAG` |
| Toolkit SBOM | GitHub Actions artifact: `sbom-toolkit-TAG` |
| Node scan results | GitHub Actions artifact: `sbom-node-TAG-scan-results` |
| Toolkit scan results | GitHub Actions artifact: `sbom-toolkit-TAG-scan-results` |
| Signed images | Attached to images in GHCR and Docker Hub |
| SBOM attestations | Attached to images in GHCR and Docker Hub |

Artifacts are retained for 90 days.

## Rollback Procedure

If issues are discovered after release:

1. **Assess severity**: Determine if rollback is needed
2. **Communicate**: Alert operators of the issue
3. **Rollback images**: Point `latest` tag to previous version
4. **Fix forward**: Create hotfix for the issue
5. **Re-release**: Push fixed images through normal pipeline

## Emergency Procedures

### Critical Vulnerability Discovered

See [Signing Runbook - Emergency Procedures](../security/signing-runbook.md#emergency-procedures)

### Signing Failure During Release

1. Check Sigstore status: https://status.sigstore.dev/
2. If transient: Re-run failed job
3. If extended outage: Document and proceed with unsigned release, re-sign later

## Related Documentation

- [Image Signing Overview](../security/image-signing.md)
- [Verification Guide](../security/verification-guide.md)
- [Signing Runbook](../security/signing-runbook.md)
