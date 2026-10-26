#toolkit

# Support compactc 0.35.0 in toolkit-js

Adds the `compact-0.35.0` variant workspace, pinning
`@midnight-ntwrk/compact-js*` 3.0.0-rc.3 and `@midnight-ntwrk/compact-runtime`
0.20.0, so `midnight-node-toolkit-js` can drive contracts compiled with
`compactc` 0.35.0 (the version `COMPACTC_VERSION` now defaults to). Registers
the version in the resolver, the root workspace dependency, and the compat
test script, and scopes a `ledger-v9` override bump to this variant only so
the existing 0.33.0/0.34.0 variants keep their pinned `ledger-v9` version.

PR: https://github.com/midnightntwrk/midnight-node/pull/2316
