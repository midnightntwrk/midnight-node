#node #try-runtime #ci

# Add `midnight-node try-runtime` upgrade dry-run and a manual CI workflow

`midnight-node try-runtime --snap <file> --runtime <wasm>`, built with `--features try-runtime`,
dry-runs a runtime upgrade against a state snapshot: migrations, pallet upgrade hooks,
multi-block migrations, a full state decode and every pallet's `try_state`.

It replaces the standalone `try-runtime-cli` for the dry-run, which lacks Midnight's
`ledger_*_bridge` host functions; snapshots are still created with `try-runtime-cli`.
Migrations that read ledger state need `--ledger-db <base-path>/ledger_storage`, as a snapshot
holds substrate storage only.

`earthly -P +try-runtime-dry-run-all [--NETWORKS=...]` runs it against the live networks,
locally or via the manual `try-runtime upgrade dry-run` workflow.

PR: https://github.com/midnightntwrk/midnight-node/pull/1523
Issue: https://github.com/midnightntwrk/midnight-node/issues/2128
