#node #keys
# Add BEEFY to the node's key definitions

`key_definitions()` now includes the BEEFY (ecdsa, `beef`) key, matching the runtime's `SessionKeys`, so the partner-chains key generation and registration flows handle it.

PR: https://github.com/midnightntwrk/midnight-node/pull/2283
Issue: https://github.com/midnightntwrk/midnight-node/issues/2284
