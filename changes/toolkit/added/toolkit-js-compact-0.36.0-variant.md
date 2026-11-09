#toolkit

# Support compactc 0.36.0 in toolkit-js

Adds the `compact-0.36.0` variant workspace, pinning
`@midnight-ntwrk/compact-js*` 3.0.0 and `@midnight-ntwrk/compact-runtime`
0.21.0, so `midnight-node-toolkit-js` can drive contracts compiled with
`compactc` 0.36.0, which `COMPACTC_VERSION` now defaults to. compact-js 3.0.0
needs the final ledger 9 packages, so the `@midnightntwrk/ledger-v9` and
`@midnightntwrk/onchain-runtime-v4` overrides move from release candidates to
1.0.0 and 4.0.0. Those are the versions the node already pins in Rust.
