#node #cnight
# Group cNIGHT observation events by Cardano transaction

The cNIGHT observation pipeline now carries events in a `CNightGroupedUtxos`
working type (one entry per Cardano transaction, sorted by position) instead
of raw sorted `Vec<ObservedUtxo>`s. The whole-transaction consensus invariant
— an inherent must never admit part of a transaction's events — becomes
structural: events enter and leave only in whole-tx units, so a call site can
no longer drop a single UTXO out of a transaction.

This also unifies the previously duplicated whole-tx truncation loops in
`truncate_to_tx_capacity` (bulk path) and `cap_whole_tx` (v2 path), which
used two subtly different definitions of "same transaction" (position
ordering vs derived equality including block hash/timestamp). "Same
transaction" is now defined once, as `(block_number, tx_index_in_block)`.

The wire format (`ObservedUtxos` / the inherent payload) is unchanged:
flattening a group is byte-identical to sorting the raw query results, and
the frozen v1 derivation path is untouched. The sliding-window cache also
stores its window as grouped transactions.

Row-limited pulls are now gap-free by construction: `bulk_pull` tracks each
category query's position frontier when it hits its row limit, cuts the
merged set at the earliest frontier, and returns that cut for the cursor to
resume from — replacing the `complete` flag whose safety relied on a
non-local `limit > max_utxos` invariant. A truncated sliding-window refresh
now shortens its coverage claim to the last whole block pulled instead of
storing a window with a hidden gap.

PR:
