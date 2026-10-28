#node #runtime #toolkit

# Bump ledger to 9.1.0.0-rc.2

Updates the midnight-ledger patch set from the 9.0.1.0-alpha.1 tags to the
9.1.0.0-rc.2 release-candidate tags (ledger-v9 crate 1.0.0).

Between alpha and rc, `midnight-coin-structure` (2.x → 3.0.0) and
`midnight-transient-crypto` (2.x → 3.0.0) went semver-major, so ledger 9 no
longer shares one crypto stack with ledgers 7/8:

- L9 gets its own workspace aliases `coin-structure-ledger-9` and
  `transient-crypto-ledger-9` (3.x); L7/L8 stay on the 2.x registry crates.
- `midnight-zkir` is patched to 2.2.0 (built against the 3.x stack); L7/L8
  helpers now prove via the renamed 2.x-stack `zkir` crate
  (tag `crate-zkir-2.1.0`), which zkir 2.2.0 itself uses internally.
- L7/L8 ledgers are built without `test-utilities` (it would pull the
  incompatible zkir); the five items the helpers used from it are vendored in
  `ledger/helpers/src/versions/test_utilities_compat.rs`.
- `ContractOperation::new` gained an `ir` argument in onchain-state 4.1.0;
  call sites go through a per-version `contract_operation_new` helper.
- Toolkit `Encoded*` conversions now have separate 2.x and 3.x impl sets.
- The `midnight-storage-core` patch is dropped (1.2.0 is on crates.io).

rc.2 also bumps the ledger-state serialization tag (v16 → v17) and adds a
`max_contract_metadata_size` ledger parameter, so build-time artifacts are
regenerated:

- `max_contract_metadata_size: 10485760` (upstream `INITIAL_LIMITS`) added to
  every `res/*/ledger-parameters-config.json`.
- Undeployed genesis state/block and all derived test fixtures rebuilt at v17
  (`+rebuild-genesis-state-undeployed`).
- Runtime metadata rebuilt (`+rebuild-metadata`).
- Devnet genesis (also v16) still needs an AWS-side rebuild + chainspec before
  the next devnet reset — deliberately not regenerated here (no seeds locally).

PR: https://github.com/midnightntwrk/midnight-node/pull/1692
