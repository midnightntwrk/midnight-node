#runtime #cnight #security

# cNIGHT observation: cost-based batch budget and block-limit fallback

The `process_tokens` inherent no longer rejects batches by UTXO count
(`CardanoTxCapacityPerBlock * 64`). A failing Mandatory inherent is
`BadMandatory`, so that check was a halt vector, not a safety net. Instead:

- New `CNightObservationApi::get_observation_cost_model` (API version 2) prices one
  observed UTXO by the pallet's own `process_tokens` weight, and a one-`Create` and a
  one-`Destroy` cNIGHT-generates-DUST update via the ledger, so the node can bound
  the batch by cost before building the inherent. Registrations and deregistrations are bounded by
  their weight; creates and spends by weight plus ledger price.
- New root call `set_observation_budget_percent` and storage
  `ObservationBudgetPercent` (default 50) set the share of the block's ledger cost
  limits one observation batch may consume, leaving headroom for the bridge and
  user transactions.
- `process_tokens` runs inside a storage layer. If the ledger still refuses the
  cNIGHT-generates-DUST update as exceeding the block limit, the batch is rolled
  back whole, `NextCardanoPosition` is left unchanged and `ObservationsDeferred` is
  emitted, so the chain keeps producing blocks instead of halting.
- The `process_tokens` weight now includes the cNIGHT-generates-DUST update's
  ledger gas, priced by kind as the batch is admitted: each `AssetCreate` adds a
  `Create` price and each `AssetSpend` a `Destroy` price. Previously the ledger
  cost of the update was not counted towards block weight.
- The count parameters leave the pallet. `CardanoTxCapacityPerBlock` storage is
  removed (it had no setter and was never written, so no migration).
  `get_utxo_capacity_per_block` is removed from `CNightObservationApi` version 2;
  nodes still call it on version 1 runtimes by its exported name.
- `CardanoBlockWindowSize` storage and `get_cardano_block_window_size` are removed.
  The window was never set and only logged a warning on large position jumps; no
  node calls the API method. No migration.

All nodes, not only validators, must run a node binary with the matching inherent
data provider change before this runtime is enacted; older binaries stop importing
blocks at the upgrade.

PR: https://github.com/shieldedtech/shadow-midnight-node/pull/45
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/599
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/590
