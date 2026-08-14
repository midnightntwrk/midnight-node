#runtime #node #toolkit #weights
# Configurable per-block transaction capacity (`tx_weight_factor_permille`)

Adds one knob, `tx_weight_factor_permille` in `res/<network>/system-parameters-config.json`,
that decides how many ledger transactions a block holds relative to the ledger's own block
limits. `1000` (the default, and what configs without the field deserialize to) is unscaled;
`500` means "half the weight per transaction", i.e. roughly twice as many per block.

A transaction's weight has two parts, and the factor reaches them by two different routes:

- The ledger-derived part is the transaction's cost *normalised against
  `LedgerParameters::limits::block_limits`*. `generate-genesis` now divides those block limits
  by the factor (`--system-parameters-config`), which widens the ledger's own per-block capacity
  and narrows this part of the weight by exactly the same ratio — so FRAME's view of a full
  block and the ledger's stay in step, and no `update-ledger-parameters` governance transaction
  is needed.
- The flat `ConfigurableTransactionSizeWeight` term does not follow the block limits, so
  `pallet_midnight` applies the factor to it directly. The chain spec seeds the pallet's new
  `TxWeightFactorPermille` storage from the same config file; `set_tx_weight_factor` (root) can
  change it on a running chain.

Genesis verification takes `--system-parameters-config` too and applies the same scaling to the
config file before comparing it against the genesis state, so the generator and the verifier
share one implementation (`midnight_node_ledger_helpers::block_capacity::scale_block_limits`).

Changing the factor requires regenerating the network's genesis state and chain spec. Intended
for test and perf networks.
