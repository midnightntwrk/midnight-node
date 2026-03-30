#node #toolkit
# Remove Default impl for WalletSeed

Remove the Default trait implementation for WalletSeed. Callers should
provide an explicit seed value.

PR: https://github.com/midnightntwrk/midnight-node/pull/804