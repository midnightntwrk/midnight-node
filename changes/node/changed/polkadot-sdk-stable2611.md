#node #runtime
# Update Polkadot SDK to polkadot-stable2611

Updates the Polkadot SDK dependency set from the Shielded fork tag `midnight-polkadot-stable2609`
to the upstream paritytech tag `polkadot-stable2611`. The fork's only patch (TCP_NODELAY on
JSON-RPC connections) is included upstream, as is the on-chain `check_inherents` fix
(paritytech/polkadot-sdk#13840).

**CLI change:** `build-spec` is removed upstream. Use `export-chain-spec` instead:

- `midnight-node build-spec --disable-default-bootnode` → `midnight-node export-chain-spec`
- `midnight-node build-spec --chain <spec> --raw --disable-default-bootnode` →
  `midnight-node export-chain-spec --chain <spec> --raw`

`export-chain-spec` never adds a default bootnode, and without `--chain` it uses the
`CFG_PRESET` configuration as `build-spec` did. The Earthfile, genesis scripts, local-env and
docs are updated; local-env still falls back to `build-spec` for older node images.

**Governance proposal hashes:** `pallet-collective` now identifies a proposal by
`hash((call, threshold))` instead of `hash(call)` (paritytech/polkadot-sdk#13259). The runtime
runs `pallet_collective::migrations::v5::MigrateToV5` for the Council and the Technical
Committee, which re-keys active motions (storage v4 -> v5). A `vote`, `close` or `kill` that
names a motion by its old hash fails with `ProposalMissing` after the upgrade. Tooling must take
the hash from the `Proposed` event or compute `hash((call, threshold))`; the toolkit's
`root-call` now reads it from the event.

Code adjustments for the new SDK API:

- `BlockLength::max_with_normal_ratio` → `BlockLength::builder().normal_ratio(..).build()`.
- The transaction pool builder returns `Arc<TransactionPoolHandle<Block>>`; the filtering pool
  wraps that instead of `TransactionPoolWrapper`.
- `CodeExecutor::call_with_execution_timeout` is implemented by the ledger context executor
  with the same on-chain marker handling as `call`.
- `sc_consensus_aura::find_pre_digest` no longer takes a signature type.
- `prost` now enables its `derive` feature explicitly, which the SDK no longer pulls in.
- `tracing-subscriber` is pinned to `=0.3.20`, the version the SDK pins.

PR: https://github.com/midnightntwrk/midnight-node/pull/2528
