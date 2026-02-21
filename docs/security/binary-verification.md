# Binary Release Verification

Midnight binary releases are cryptographically signed using [Sigstore](https://www.sigstore.dev/) keyless signing. This allows operators and SPOs to verify that binaries were legitimately built by Midnight's CI/CD pipeline.

## Quick Start

Verify a binary with a single command:

```bash
./scripts/verify-binary.sh midnight-node-node-1.0.0-linux-amd64.tar.gz
```

## Prerequisites

Install [cosign](https://docs.sigstore.dev/cosign/system_config/installation/):

```bash
# macOS
brew install cosign

# Linux (download binary)
curl -sSfL https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64 -o cosign
chmod +x cosign
sudo mv cosign /usr/local/bin/

# Go
go install github.com/sigstore/cosign/v2/cmd/cosign@latest
```

## What Gets Verified

### Binary Signatures

Every Midnight binary release is signed during the CI/CD build process using GitHub Actions' OIDC identity. The signature proves:

- The binary was built by the `midnightntwrk/midnight-node` GitHub repository
- The build ran on GitHub Actions infrastructure
- The binary contents have not been tampered with since signing

### Checksums

Each release includes a signed `SHA256SUMS` file containing checksums for all release artifacts. This provides:

- Integrity verification for downloaded files
- Protection against download corruption
- Cryptographic proof the checksums were generated during the official build

## Signed Binaries

The following binaries are signed in each release:

| Binary | Platforms |
|--------|-----------|
| `midnight-node` | linux-amd64, linux-arm64 |
| `midnight-node-toolkit` | linux-amd64, linux-arm64 |

Release artifacts follow this naming convention:
- `midnight-node-node-{VERSION}-linux-{PLATFORM}.tar.gz`
- `midnight-node-toolkit-node-{VERSION}-linux-{PLATFORM}.tar.gz`

Each binary has accompanying signature files:
- `{BINARY}.sig` - Detached signature
- `{BINARY}.pem` - Signing certificate

## Usage Examples

### Basic Signature Verification

```bash
# Download release files
curl -LO https://github.com/midnightntwrk/midnight-node/releases/download/node-1.0.0/midnight-node-node-1.0.0-linux-amd64.tar.gz
curl -LO https://github.com/midnightntwrk/midnight-node/releases/download/node-1.0.0/midnight-node-node-1.0.0-linux-amd64.tar.gz.sig
curl -LO https://github.com/midnightntwrk/midnight-node/releases/download/node-1.0.0/midnight-node-node-1.0.0-linux-amd64.tar.gz.pem

# Verify signature
./scripts/verify-binary.sh midnight-node-node-1.0.0-linux-amd64.tar.gz
```

### With Checksum Verification

```bash
# Also download SHA256SUMS
curl -LO https://github.com/midnightntwrk/midnight-node/releases/download/node-1.0.0/SHA256SUMS

# Verify signature and checksum
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

For advanced users who want to run cosign directly:

### Verify Signature

```bash
cosign verify-blob \
    --certificate midnight-node-node-1.0.0-linux-amd64.tar.gz.pem \
    --signature midnight-node-node-1.0.0-linux-amd64.tar.gz.sig \
    --certificate-identity-regexp "https://github.com/midnightntwrk/midnight-node/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    midnight-node-node-1.0.0-linux-amd64.tar.gz
```

### One-Liner Verification (Without Downloading Script)

For users who want to verify without cloning the repository:

```bash
# Download and verify in one command
RELEASE="node-1.0.0"
BINARY="midnight-node-${RELEASE}-linux-amd64.tar.gz"
BASE_URL="https://github.com/midnightntwrk/midnight-node/releases/download/${RELEASE}"

cosign verify-blob \
    --certificate <(curl -sL "${BASE_URL}/${BINARY}.pem") \
    --signature <(curl -sL "${BASE_URL}/${BINARY}.sig") \
    --certificate-identity-regexp "https://github.com/midnightntwrk/midnight-node/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    "$BINARY"
```

### Verify Checksums File

```bash
# Verify the SHA256SUMS file itself is signed
cosign verify-blob \
    --certificate SHA256SUMS.pem \
    --signature SHA256SUMS.sig \
    --certificate-identity-regexp "https://github.com/midnightntwrk/midnight-node/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    SHA256SUMS

# Then verify individual files against checksums
sha256sum -c SHA256SUMS
```

## Troubleshooting

### "cosign is not installed"

**Cause:** The cosign tool is not installed or not in PATH.

**Solutions:**
- Install cosign using one of the methods in Prerequisites
- Ensure the cosign binary is in your PATH

### "Signature file not found"

**Cause:** The `.sig` file is missing.

**Solutions:**
- Download the signature file from the GitHub release
- Ensure all three files (binary, .sig, .pem) are in the same directory

### "Certificate file not found"

**Cause:** The `.pem` file is missing.

**Solutions:**
- Download the certificate file from the GitHub release
- Ensure all three files (binary, .sig, .pem) are in the same directory

### "Signature verification failed"

**Cause:** The signature doesn't match the binary or certificate.

**Possible reasons:**
- Binary file was modified or corrupted during download
- Signature file is for a different binary
- Binary was not built by official CI/CD

**Solutions:**
- Re-download all files from the official GitHub release
- Verify you're downloading from `github.com/midnightntwrk/midnight-node`
- Check file sizes match what's shown on the release page

### "Certificate identity mismatch"

**Cause:** The binary was not built by Midnight's official CI/CD.

**Solutions:**
- Verify you're using an official Midnight release
- Check if the binary was built from a fork or unofficial source
- Contact the Midnight team if you believe this is an error

### "OIDC issuer mismatch"

**Cause:** The binary was not built on GitHub Actions.

**Solutions:**
- This indicates a non-official build
- Use official binaries from GitHub releases

### "Checksum verification failed"

**Cause:** The file's checksum doesn't match the expected value.

**Solutions:**
- Re-download the binary file
- Verify the SHA256SUMS file is from the same release
- Check for download corruption

## Security Considerations

- **Always verify before deploying:** Especially in production environments
- **Use specific versions:** Avoid using unverified binaries
- **Verify the checksums file:** The SHA256SUMS file itself is signed
- **Automate verification:** Include verification in deployment scripts
- **Monitor for failures:** Alert on verification failures
