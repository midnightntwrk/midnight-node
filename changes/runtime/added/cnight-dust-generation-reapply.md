#cnight #dust #migration
# Re-apply cNIGHT dust generation after the ledger 8 -> 9 hardfork

The v8 -> v9 state translation now drops the ledger's dust state and installs the
empty one genesis starts from (the toolkit's `fork_context_8_to_9` mirrors it,
resetting each wallet's local dust state). That would silently stop DUST
generation for every cNIGHT holder, so `pallet-cnight-observation` rebuilds its
own slice of the generating set as a multi-block migration (storage version
1 -> 2):

- `on_runtime_upgrade` saves the pre-fork ledger-8 arena root — the only place the
  wiped entries' night value and dust owner survive;
- the MBM pages through `UtxoOwners`, reads each nonce's pre-wipe value and owner
  through a new `dust_generation_values_v8` host function, and re-applies them in
  batches of 25 as `CNightGeneratesDustUpdate` system transactions;
- Cardano observations are ignored while it runs (the existing storage version
  gate in `process_tokens`), so `NextCardanoPosition` does not advance and the
  observer re-delivers the same UTXOs afterwards.

Each batch is priced before it is applied — `get_transaction_cost` (version 2)
now prices system transactions too — and applied only if it fits the remaining
MBM weight budget; otherwise the step hands its cursor back and the next block
retries the page. A batch too large for a whole block's budget is given up on,
emitting `DustReapplySkipped` and lifting the gate. Ledger work applied earlier in
the same block (notably the C2M bridge inherent) can still make a priced batch
exceed `block_limits`; a new `is_block_limit_exceeded` hook on
`MidnightSystemTransactionExecutor` distinguishes that from a real rejection, so
the batch is *deferred* unmoved rather than skipped. Restoration runs at ~175
nonces/block on current parameters; measured live sets on 2026-08-06 were 4870
(mainnet), 1524 (preview), 85 (preprod) — ~28, ~9 and 1 block of gated
observation.

Restored entries are field-for-field identical to the wiped ones except the
accrual clock: the original creation time lived on the destroyed dust UTXO, so the
replay stamps `fork block time - dust.time_to_cap()` (~1 week), putting every
holder back at their cap on landing rather than at zero refilling over a week.

**Only cnight's slice is restored.** Native NIGHT registers generation entries
too, and nothing in this repo records which of those the wipe took, so
`DustReapplyCompleted` must not be read as "all dust generation restored".

## Warp sync during the replay

A node beginning warp sync to a block inside the replay window cannot read the
pre-fork arena root (it is not in the v9-rooted snapshot), so it would cancel the
replay locally and diverge. `warp_ledger_sync`'s recovery monitor now checks
`CNightObservation::PreForkStateKey` at the warp target and, if set, leaves the
recovery gate armed — holding authoring and import with a logged explanation. This
is terminal for that database; re-sync from a purged one once the replay has
finished. Warp sync new nodes before the upgrade or after `DustReapplyCompleted`,
not in between.

## Watching it happen

New events: `DustReapplyStarted`, `DustReapplyBatchFailed`,
`DustReapplyCompleted`, `DustReapplySkipped`, and
`ObservationsSkippedForMigration` (once per block while the gate holds).

Only `DustReapplyStarted` and `ObservationsSkippedForMigration` show up in
Polkadot.js Apps. The rest are deposited from an MBM step, whose phase is
`ApplyExtrinsic(n)` for an `n` no extrinsic in the block has, and the explorer
drops events it cannot attach to an extrinsic. Read them via `System::Events` or
subxt's `events()`, or grep the node log — each is logged under its own name. The
same quirk is why the toolkit's block fetcher, which collected system transactions
only while walking the matching extrinsic, would have dropped every replayed
batch; it now keys on the event's own phase and sorts the block's transactions
into execution order.

PR: https://github.com/midnightntwrk/midnight-node/pull/2012
Issues:
- https://github.com/shieldedtech/shielded-security-engineering/issues/548
- https://github.com/shieldedtech/shielded-security-engineering/issues/549
- https://github.com/shieldedtech/shielded-security-engineering/issues/550
