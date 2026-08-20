# Historical Ledger 8 verifier

The indexer replays blocks using the verifier version that accepted them when
they were produced. These local crates preserve `midnight-ledger` and
`midnight-zswap` 8.1.0 inside the indexer's independently locked worker build.
The node's workspace patches therefore cannot alter the historical verifier's
storage, serialization, or cryptography dependencies.

The source is copied from the Apache-2.0 crates.io packages published from
`midnightntwrk/midnight-ledger` commit
`d89e0b6334f83bc9477152fb5edf7eca71660237`:

- `midnight-ledger` 8.1.0, checksum
  `2182054f3a43ccabac514448fff2487437be293f517a01afc23905df701e8548`
- `midnight-zswap` 8.1.0, checksum
  `4c83f946d8ac03abea38ca470599801fa08e91e2ddf3cb0963027a7701180c7c`

The verifier implementation is unchanged. Cargo cache markers and member
lockfiles are omitted, fixture whitespace is normalized, and the only
functional packaging change is that `ledger-v8/Cargo.toml` points its `zswap`
dependency at the adjacent local `zswap-v8` crate.
