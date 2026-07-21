#node

# Document the non-ledger transaction surface in rustdoc

Adds crate- and item-level documentation across the node crates that define the
non-ledger transaction surface: `TransactionType`/`TransactionTypeV2` and their
variants in `primitives/midnight`, the `send_mn_transaction` envelope in
`pallets/midnight`, the system-transaction entry points and
`SystemTransactionApplied` event in `pallets/midnight-system`, the
`process_tokens` observation-inherent path in `pallets/cnight-observation`, and
the bridge-transfer framing in `pallets/c2m-bridge`. The docs state the
node/ledger boundary, frame the transaction classification as an off-node
vocabulary consumed via runtime metadata (not in-node dispatch), and remove a
stale FRAME-template scaffold comment and `//todo` placeholder. Documentation
only — no behaviour, signatures, or error codes change.

Closes: https://github.com/midnightntwrk/midnight-node/issues/1533
PR: https://github.com/midnightntwrk/midnight-node/pull/1619
