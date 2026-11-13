#runtime #cnight-observation
# A cNIGHT observation batch that cannot be built into a ledger update rolls back

`CNightObservation::process_tokens` now rolls the whole batch back and emits
`ObservationsDeferred` when a spend's DUST event, or the batch's system transaction, cannot
be constructed. `NextCardanoCursor` stays put, so observation stalls at that position instead
of moving past it.

Before, both failures were logged and the batch was committed: the cursor advanced, the
spend's `UtxoOwners` entry was removed, and the ledger never received the update. Neither
failure is expected: a spend rebuilds its event from an owner key that already built one on
create.

A create whose owner key is not a valid DUST public key is still skipped. Registration
datums are user input, and no owner is recorded for it.

PR: https://github.com/midnightntwrk/midnight-node/pull/2442
