#ci
# Skip SBOM attestation for non-release CI builds

Non-release CI builds (PR, main merge) now skip SBOM attestation entirely
instead of attempting `--tlog-upload=false`, which cosign v3.0.2 does not
respect during bundle signing. SBOM generation and vulnerability scanning
still run. Release builds retain full attestation with transparency log
upload for supply-chain assurance.

PR: https://github.com/midnightntwrk/midnight-node/pull/781
