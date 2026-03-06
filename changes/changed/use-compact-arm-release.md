#ci

# Use native ARM64 compactc, remove QEMU emulation from CI

Now that compactc releases arm64 binaries, CI builds use native compactc on arm64 runners
instead of emulating x86_64 via QEMU. The contract precompile image workflow builds for
both amd64 and arm64 natively. Also consolidates Node.js, subxt-cli, and Docker into the
CI base image to reduce per-target installation overhead.

PR: https://github.com/midnightntwrk/midnight-node/pull/826
JIRA: https://shielded.atlassian.net/browse/PM-21000
