# Binary Release Verification

Midnight binary releases are attested using [GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations). This allows operators and SPOs to verify that binaries were legitimately built by Midnight's CI/CD pipeline.

## Quick Start

Verify a binary with a single command:

```bash
./scripts/verify-binary.sh midnight-node-node-1.0.0-linux-amd64.tar.gz
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

Every Midnight binary release has a build provenance attestation created during CI/CD using `actions/attest-build-provenance`. The attestation proves:

- The binary was built by the `midnightntwrk/midnight-node` GitHub repository
- The build ran on GitHub Actions infrastructure
- The binary contents have not been tampered with since building

### Checksums

Each release includes an attested `SHA256SUMS` file containing checksums for all release artifacts. This provides:

- Integrity verification for downloaded files
- Protection against download corruption
- Cryptographic proof the checksums were generated during the official build

## Attested Binaries

The following binaries are attested in each release:

| Binary | Platforms |
|--------|-----------|
| `midnight-node` | linux-amd64, linux-arm64 |
| `midnight-node-toolkit` | linux-amd64, linux-arm64 |

Release artifacts follow this naming convention:
- `midnight-node-node-{VERSION}-linux-{PLATFORM}.tar.gz`
- `midnight-node-toolkit-node-{VERSION}-linux-{PLATFORM}.tar.gz`

## Usage Examples

### Basic Attestation Verification

```bash
# Download release binary
curl -LO https://github.com/midnightntwrk/midnight-node/releases/download/node-1.0.0/midnight-node-node-1.0.0-linux-amd64.tar.gz

# Verify build provenance
./scripts/verify-binary.sh midnight-node-node-1.0.0-linux-amd64.tar.gz
```

### With Checksum Verification

```bash
# Also download SHA256SUMS
curl -LO https://github.com/midnightntwrk/midnight-node/releases/download/node-1.0.0/SHA256SUMS

# Verify build provenance and checksum
./scripts/verify-binary.sh --checksum midnight-node-node-1.0.0-linux-amd64.tar.gz
```

### Scripted Use

```bash
# Quiet mode for CI/CD pipelines (exit code only)
if ./scripts/verify-binary.sh --quiet midnight-node-node-1.0.0-linux-amd64.tar.gz; then
    echo "Binary verified successfully"
else
    echo "Binary verification failed"
    exit 1
fi
```

## Manual Verification

For advanced users who want to run `gh` directly:

### Verify Build Provenance

```bash
gh attestation verify midnight-node-node-1.0.0-linux-amd64.tar.gz \
    --repo midnightntwrk/midnight-node
```

### One-Liner Verification (Without Downloading Script)

For users who want to verify without cloning the repository:

```bash
RELEASE="node-1.0.0"
BINARY="midnight-node-node-1.0.0-linux-amd64.tar.gz"

gh attestation verify "$BINARY" --repo midnightntwrk/midnight-node
```

### Verify Checksums File

```bash
# Verify the SHA256SUMS file itself is attested
gh attestation verify SHA256SUMS --repo midnightntwrk/midnight-node

# Then verify individual files against checksums
sha256sum -c SHA256SUMS
```

## Troubleshooting

### "gh CLI is not installed"

**Cause:** The GitHub CLI is not installed or not in PATH.

**Solutions:**
- Install `gh` using one of the methods in Prerequisites
- Ensure the `gh` binary is in your PATH

### "no attestations found"

**Cause:** The binary doesn't have an attestation.

**Solutions:**
- Verify you're downloading from the official `midnightntwrk/midnight-node` GitHub releases
- Check if the binary predates attestation implementation
- Re-download the binary from the official release

### "verification failed"

**Cause:** The attestation doesn't match the binary.

**Possible reasons:**
- Binary file was modified or corrupted during download
- Binary was not built by official CI/CD

**Solutions:**
- Re-download the binary from the official GitHub release
- Verify you're downloading from `github.com/midnightntwrk/midnight-node`
- Check file sizes match what's shown on the release page

### "Checksum verification failed"

**Cause:** The file's checksum doesn't match the expected value.

**Solutions:**
- Re-download the binary file
- Verify the SHA256SUMS file is from the same release
- Check for download corruption

## Security Considerations

- **Always verify before deploying:** Especially in production environments
- **Use specific versions:** Avoid using unverified binaries
- **Verify the checksums file:** The SHA256SUMS file itself is attested
- **Automate verification:** Include verification in deployment scripts
- **Monitor for failures:** Alert on verification failures
