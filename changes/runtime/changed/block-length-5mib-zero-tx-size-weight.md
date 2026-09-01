#performance #weights
# Raise the block length limit to 5 MiB and zero the flat per-transaction weight add-on

Two independent throughput limits were what closed a block long before the node ran out
of execution capacity. Both are lifted here.

**Block length: 1 MiB -> 5 MiB.** `frame_system`'s `BlockLength` was
`max_with_normal_ratio(1 MiB, 75%)`, i.e. ~786 KiB usable for normal dispatches. Midnight
transactions are large relative to a typical Substrate extrinsic, so blocks were hitting
the *length* limit rather than the weight limit.

**`ConfigurableTransactionSizeWeight` now defaults to `Weight::zero()`.** It shared
`DefaultWeight` with `ConfigurableOnInitializeWeight` and
`ConfigurableOnRuntimeUpgradeWeight`, so its default was `EXTRA_WEIGHT_TX_SIZE`
(20 ms ref-time, ~1% of a 2 s block) added flat to *every* transaction on top of its real
gas-metered cost. That add-on, not the metered cost, was the dominant term in a
transaction's weight, capping block fullness far below what the node can execute. It now
gets its own `#[pallet::type_value]` returning zero.

`DefaultWeight` and the `get_tx_weight` fallback for a transaction whose cost cannot be
metered are deliberately unchanged — they still use `EXTRA_WEIGHT_TX_SIZE`. This changes
exactly the flat per-transaction add-on and nothing else. The value remains settable at
runtime via the root-only `set_tx_size_weight` extrinsic, so an existing chain that has
already zeroed it by extrinsic sees no change, and a chain on the old default gets the new
one only for a fresh genesis (a storage default applies where no value is stored).

Both values are metadata-visible (`BlockLength` is a `frame_system` constant, the weight
default is a storage-entry default), so runtime metadata needs rebuilding.

`spec_version` bumped 002_001_000 -> 002_001_001.
