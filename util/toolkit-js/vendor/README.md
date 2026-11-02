# Vendored ledger wasm packages

Pre-release builds of the ledger-9 wasm packages matching the Rust ledger pins in `Cargo.lock`,
built with `nix build .#ledger-wasm` and `nix build .#onchain-runtime-wasm` from the ledger source
at those pins. They exist here only because no matching build is published yet: the node's VM
charges contract transcripts under the pinned ledger's rules, and the published packages still
compute transcript gas under the previous rules, so every contract transaction is rejected with
`ran out of gas budget` without them.

`package.json` `overrides` point at these tarballs. Remove the directory and restore registry
versions in `overrides` once the matching packages are published.

| File | SHA-256 |
| ---- | ------- |
| `midnight-ledger-v9-1.0.1-rc.2.tgz` | `c3abaf0de1686dfa2a4f06878a0b0846b3acb1767fa657c137d6460f2519d8cc` |
| `midnight-onchain-runtime-v4-4.0.1-rc.2.tgz` | `155f2469b250310c1e50dce992c35c4102cb9730f67b8afbb0f26fdbd25b57bb` |
