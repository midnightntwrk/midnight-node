#ci

# Migrate image signing from Cosign/Sigstore to GitHub native attestations

Replace Cosign keyless signing with GitHub's `actions/attest-build-provenance` and
`actions/attest-sbom` actions, eliminating external Sigstore/Rekor dependencies.
Re-enable SBOM attestation (previously disabled due to Rekor rejecting payloads).
Add build provenance attestation for release binary assets.

PR: https://github.com/midnightntwrk/midnight-node/pull/786
