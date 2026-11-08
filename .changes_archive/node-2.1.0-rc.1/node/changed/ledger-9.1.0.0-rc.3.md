#node #runtime #toolkit

# Bump ledger to 9.1.0.0-rc.3

Updates the midnight-ledger patch set from the 9.1.0.0-rc.2 tags to the
9.1.0.0-rc.3 release-candidate tags.

rc.3 keeps the L7/L8 (2.x) vs L9 (3.x) crypto-stack split from rc.2 but folds
both stacks into one zkir crate, so the node-side wiring changes:

- Two new `[patch.crates-io]` entries: `midnight-base-crypto-derive` (base-crypto
  split out a proc-macro companion crate) and `midnight-transient-crypto-old`
  (`package = "midnight-transient-crypto"`, tag `transient-crypto-2.2.0-rc.1`).
  Both `midnight-zkir` 2.2.0-rc.3 and `midnight-zswap` 9.0.0-rc.3 now require
  `transient-crypto-old ^2.2.0`, and the registry only publishes up to 2.1.0, so
  the git 2.2.0 patch is mandatory.
- The rc.2-era renamed `zkir-2-1` workspace dep (tag `crate-zkir-2.1.0`) is
  retired. rc.3's `midnight-zkir` 2.2.0 carries both the 3.x stack and
  `transient-crypto-old` and proves V0/V1 (old) and V2 circuits in one crate, so
  L7/L8/L9 all use the single workspace `zkir`. The frozen 2.1.0 crate could not
  satisfy transient-crypto 2.2.0's new `Zkir` trait methods anyway.
- `verifier_key()` was dropped from upstream `test_utilities` in rc.3; it is now
  provided in `ledger/helpers/.../common` (a thin `resolve_key` + deserialize),
  so it works across all ledger versions.
- `SingleUpdate` gained `IrInsert`/`IrRemove` (on-chain IR maintenance). The
  shared transaction-details match handles them via a cross-version wildcard
  (the variants exist only in L9, and the match is compiled per ledger version);
  the functional apply lives in the ledger crate.

rc.3 also bumps the ledger-state serialization tag (v17 → v18) and replaces the
`parallelism_factor` cost-model parameter with three `FixedPoint` factors:

- `parallelism_factor: 4` → `validation_factor: 0.25`, `guaranteed_factor: 1.0`,
  `fallible_factor: 1.0` (the upstream carry-over for the old `/4`). Applied only
  to the networks on the new ledger — `res/dev` (also used by `undeployed`),
  `res/devnet`, `res/stagenet`. Deployed networks (govnet, mainnet, perfnet,
  preprod, preview, qanet) read via L7/L8 and keep `parallelism_factor`.
- Undeployed genesis (`res/genesis/genesis_{block,state}_undeployed.mn`, mirrored
  into the toolkit test-data fixtures) and the derived `.mn` test fixtures
  (`res/test-tx-deserialize`, `res/test-zswap`, `res/test-contract/contract_tx_*`)
  are rebuilt at v18. Runtime metadata is *not* regenerated — the change is to
  ledger-parameter config plus ledger-state encoding, not pallet storage or
  extrinsics. Devnet genesis still needs an AWS-side rebuild + chainspec before the
  next devnet reset (no seeds locally), so it is deliberately not regenerated here;
  deployed-network genesis is unchanged.

PR: https://github.com/midnightntwrk/midnight-node/pull/1738
Issue: https://github.com/midnightntwrk/midnight-node/issues/1737
