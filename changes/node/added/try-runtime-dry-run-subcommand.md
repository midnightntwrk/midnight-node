#node #try-runtime #ci

# Add `midnight-node try-runtime` upgrade dry-run and a manual CI workflow

`midnight-node try-runtime --snap <file> --runtime <wasm>` (built with
`--features try-runtime`) dry-runs a runtime upgrade against a state snapshot:
`Migrations` plus every pallet's `on_runtime_upgrade`, then the multi-block
migration queue, then `try_decode_entire_state` and every pallet's `try_state`.

It is in-tree rather than the standalone `try-runtime-cli` because that links only
`sp_io::SubstrateHostFunctions` and cannot resolve Midnight's `ledger_*_bridge` host
functions, so the upgrade traps before any check runs. Snapshot creation still uses
`try-runtime-cli`, which needs none.

Earthly targets `+try-runtime-build` and
`+try-runtime-dry-run-<preview|preprod|mainnet>` drive it, so the same command runs
locally and in the new manual `try-runtime upgrade dry-run` workflow.

A snapshot carries substrate storage only — the ledger arena lives in its own
database — so migrations that read ledger state need `--ledger-db` pointing at a
node's `ledger_storage`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1523
Issue: https://github.com/midnightntwrk/midnight-node/issues/2128
