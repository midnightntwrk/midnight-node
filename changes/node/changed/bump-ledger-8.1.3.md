#node #ledger #security
# Bump ledger 8 to 8.1.3

Security patch, node-only: no runtime change. Ledger 8 executes in the node's
host functions, and the runtime's wasm dependency graph does not include any
of the bumped crates.

- contract call transcripts must embed canonical field values: a `field` atom
  encoding `x + p` is rejected in `push`, `pushs` and `idx`-style path keys
- `noop 0` in a contract call transcript is rejected as not normalized

An 8.1.3 node rejects contract calls an 8.1.2 node accepts, so block producers
should upgrade together.

Crate versions: `midnight-ledger`/`midnight-zswap` 8.1.3,
`midnight-onchain-runtime` 3.1.2, `midnight-onchain-state` 3.0.2,
`midnight-onchain-vm` 3.1.2.

Backport of the ledger bump from #2238 (release/node-1.0.400).

PR: <link to PR>
