#toolkit #security
# Resolve ledger versions from Cargo.lock at build time

The toolkit `version` command now reports the *resolved* ledger crate versions, baked in at
compile time from `Cargo.lock` by a `build.rs` (`cargo metadata --locked`), instead of
parsing the requested specs from `Cargo.toml` at runtime. The manifest can hide the built version:
a git dependency pinned by `version = "=1.0.0"` actually resolves to a specific pre-release
tag/commit. Git deps now report their locked tag and full commit SHA, e.g.
`Ledger: 9 (1.0.0 (tag: crate-ledger-9.1.0.0-rc.3, rev: 85e769a0...))`. Reading the resolve graph keys
each dependency by its workspace alias, so the two aliases that rename `midnight-ledger` at
different versions stay distinct with no version-spec guesswork. Addresses Least Authority audit
finding "Prefer Cargo.lock For Build-Time Crate Versions".

PR: https://github.com/midnightntwrk/midnight-node/pull/1793
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/330
