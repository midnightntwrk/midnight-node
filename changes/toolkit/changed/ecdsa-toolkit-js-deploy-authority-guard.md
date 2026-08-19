#toolkit #ledger #ecdsa #contract-maintenance #toolkit-js

# toolkit-js deploy authority seed accepts the scheme prefix (ECDSA rejected)

`generate-intent deploy --authority-seed` now accepts the same optional `schnorr:`/`ecdsa:` scheme
prefix as every other seed flag (bare = Schnorr, backwards compatible), instead of a bare seed only.

Unlike the native `generate-txs contract-simple deploy` path, the toolkit-js deploy path cannot
actually build an ECDSA contract-maintenance authority: it hands the authority key to the
`@midnight-ntwrk/compact-js` toolchain as a value-only `--signing` argument, and that toolchain has
no channel to carry the ledger-9 signature scheme (`signingKind`) through to the deployed authority —
an ECDSA seed would silently deploy a Schnorr authority. So an `ecdsa:` seed is now **rejected up
front**, before any toolkit-js process is spawned, with an error pointing at the native
`generate-txs contract-simple deploy --authority-seed ecdsa:<seed>` path (which fully supports ECDSA
and mixed-scheme committees). Schnorr behaviour is unchanged.

This keeps the seed-flag surface consistent across the CLI and turns what used to be a confusing hex
parse error (`ecdsa:` is not valid hex) into a clear, actionable message.

Tests: unit coverage that an `ecdsa:` authority seed is rejected before spawning toolkit-js and that
a Schnorr seed passes the scheme guard (`util/toolkit`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1861
Issue: https://github.com/midnightntwrk/midnight-node/issues/1542
