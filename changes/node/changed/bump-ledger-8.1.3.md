#node #ledger #security
# Bump ledger 8 to 8.1.3 and ledger 9 to 9.1.0.0-rc.6

Security patch, node-only: no runtime change. Ledger 8 and ledger 9 execute in
the node's host functions, and the runtime's wasm dependency graph does not
include any of the bumped crates.

- contract call transcripts must embed canonical field values: a `field` atom
  encoding `x + p` is rejected in `push`, `pushs` and `idx`-style path keys
- `noop 0` in a contract call transcript is rejected as not normalized

Ledger 9.1.0.0-rc.6 carries the same fixes (and the 8.1.2 hardening) into
ledger 9; the two have to move together because they share
`midnight-base-crypto`.

An updated node rejects contract calls an older node accepts, so block
producers should upgrade together.

Crate versions: `midnight-ledger`/`midnight-zswap` 8.1.3,
`midnight-onchain-runtime` 3.1.2, `midnight-onchain-state` 3.0.2,
`midnight-onchain-vm` 3.1.2; ledger 9 per-crate tags `crate-ledger-9.1.0.0-rc.6`,
`base-crypto-1.1.0-rc.4`, `onchain-vm-4.0.0-rc.4`, `onchain-state-4.0.0-rc.5`,
`crate-onchain-runtime-4.0.0-rc.5`; `midnight-proofs` 0.8.2.

Same change on release/node-2.1.0: #2254. Ledger 8 bump on release/node-1.0.400: #2238.

PR: https://github.com/midnightntwrk/midnight-node/pull/2257
