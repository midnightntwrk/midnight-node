#toolkit
# Vendor the ledger-9 wasm packages matching the pinned ledger

`util/toolkit-js` now overlays pre-release builds of `@midnightntwrk/ledger-v9` and
`@midnightntwrk/onchain-runtime-v4` (built from the same ledger source the node pins) onto the
installed packages in a `postinstall` step. The published packages compute contract transcript gas
under the previous ledger's rules, so every contract transaction they build is rejected by the
node with `ran out of gas budget`. Temporary until the matching packages are published.

PR: https://github.com/shieldedtech/shadow-midnight-node/pull/85
