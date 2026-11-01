#node #ledger #security #hardfork
# Bump ledger 9 to 9.1.0.1-rc.2 and split its storage from ledger 8

Ledger 9 moves to the 9.1.0.1-rc.2 per-crate tags: `crate-ledger-9.1.0.1-rc.2`,
`zswap-9.0.1-rc.2`, `coin-structure-3.0.1-rc.1`, `base-crypto-1.1.1-rc.2`,
`transient-crypto-3.0.1-rc.1`, `onchain-vm-4.0.1-rc.2`, `onchain-state-4.0.1-rc.1`,
`crate-onchain-runtime-4.0.1-rc.2`, `serialize-1.2.1-rc.1`, `storage-3.0.0-rc.1`,
`storage-core-1.2.3-rc.2`, `storage-macros-1.0.1-rc.1`. Ledger 8 stays on crates.io 8.1.3.

Security fixes carried by the ledger: transaction pricing and well-formedness run in
time linear in the encoding (repeated entries are rejected first), contracts hold at most
256 operations, each deploy is charged for its full state, transcripts declaring gas beyond
the block limits or Dust transfers are malformed, zswap offers verify each distinct entry
once, writes are priced per segment, and trie decoding checks keys once per node.

`midnight-storage` went semver-major (3.0.0) for ledger 9, so the node now binds two storage
crates: `ledger-storage-ledger-8` (2.0.3, crates.io) and `ledger-storage-ledger-9` (3.0.0).
Both share one `midnight-storage-core`, hence one parity-db arena, and `storage-core` is
back on sha2 0.10 - the arena hasher is named through the `storage-sha2` alias so the node
cannot drift from it. The v8->v9 state translation crate is pinned to its storage-3 port,
whose translation machinery now lives on the v9 storage side.

PR: https://github.com/shieldedtech/shadow-midnight-node/pull/85
