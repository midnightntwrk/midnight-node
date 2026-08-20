#cnight #dust #migration
# Re-apply cNIGHT dust generation after the ledger 8 -> 9 hardfork

The ledger 8 -> 9 hardfork wipes dust state, which would silently stop DUST
generation for every cNIGHT holder. `pallet-cnight-observation` now rebuilds its
own slice of the ledger's dust generating set as a multi-block migration
(storage version 1 -> 2):

- a single-block migration run in the upgrade block saves the pre-fork ledger-8
  arena root, the only place the wiped entries' night value and dust owner
  survive;
- the multi-block migration then pages through `UtxoOwners` (the set of nonces
  that are cnight's and still live), reads each nonce's pre-wipe value and owner
  through a new `dust_generation_values_v8` host function (which also serves the
  state's dust `time_to_cap`), and re-applies them in batches of 25 as
  `CNightGeneratesDustUpdate` system transactions;
- incoming Cardano observations are ignored while it runs — the existing storage
  version gate in `process_tokens` covers it, so `NextCardanoPosition` does not
  advance and the observer re-delivers the same UTXOs afterwards.

Each batch is priced from the ledger's own cost model *before* it is applied.
`process_tokens`' benchmark observes registration UTXOs, which never reach the
ledger, so its weight says nothing about what dust `Create`s cost; instead the
migration asks the ledger what the batch it has built is worth — a widened
`get_transaction_cost` host function (version 2) that also prices system
transactions, not just user ones — and applies it only if the price fits what is left
of the multi-block-migration weight budget. `SystemTransaction::cost` is pure, so
pricing ahead of applying costs nothing, and a step can never overrun its budget and
then report having stayed inside it. A batch that does not fit is not applied at all:
the step hands its cursor straight back and the next block retries that same page on a
fresh budget. `pallet_migrations` runs exactly one step per block, so that packing
loop is what sets throughput.

A batch priced above what the migration may spend in a *whole* block gives up rather
than retrying: a page is only 25 nonces, so no fresh budget would ever afford it. The
replay emits `DustReapplySkipped` and bumps the storage version so observations resume,
leaving whatever it had already restored in place.

Nothing here is hardcoded: the figure comes from `SystemTransaction::cost`
normalized against the ledger's own `parameters.limits.block_limits`, so the
migration paces itself to whatever limits a given network reports, and stays correct
if `OverwriteParameters` moves them. It also means a block that stays inside the MBM
budget keeps the migration's own share inside the ledger's per-block fullness
accounting. On the current
parameters a dust `Create` prices at ~8.96e9 `ref_time`, so 80% of a 2e12 block
affords 178 — 7 batches of 25, i.e. 175 restored per block. Measured live sets on
2026-08-06: **mainnet 4870** nonces (finalized #2019697), **preview 1524**
(#301994), **preprod 85** (#1985972) — so ~28 blocks (~2.8 min) of gated
observation on mainnet, ~9 on preview, 1 on preprod.

The batch size is a packing granularity, not a limit: smaller batches fill the
budget more tightly (7 x 25 = 175 against 3 x 50 = 150) and keep the blast radius
of a failed batch small, at the price of one extra host call and one extra system
transaction each. A batch the ledger cannot price falls back to the previous
half-a-block charge, so an unpriceable batch costs latency, never a stall.

The restored generation entries are field-for-field identical to the wiped ones.
Only the accrual clock moves: the original creation time lives on the dust UTXO
the wipe takes, so the replay stamps `fork block time - dust.time_to_cap()`
(~1 week). DUST accrues linearly from the creation time to a cap of
`night_value * night_dust_ratio` reached after `time_to_cap`, so backdating by
exactly that much puts every holder at their cap the moment the replay lands —
the pre-fork steady state, since anyone who had held cNIGHT for a week was
already capped. Stamping the fork block itself would instead start everyone at
zero and refill over a week in proportion to holdings, locking small holders out
of paying fees for days. The real per-UTXO creation time is only available from
db-sync, and would restore holders to the same cap anyway for all but the
youngest UTXOs, at the price of a new consensus-critical mainchain query in the
middle of the hardfork; the chosen offset over-credits only cNIGHT locked in the
last week, bounded by a cap it would have reached regardless.

**Only cnight's slice of the generating set is restored.** Native NIGHT registers
generation entries too (at dust registration, for delegated unshielded outputs,
and on the mint/claim path); nothing in this repo records which of those the wipe
took, so `DustReapplyCompleted` must not be read as "all dust generation
restored".

The wipe itself is part of this change: the v8 -> v9 state translation now drops
the dust state and installs the empty one genesis starts from, instead of
carrying it across. The toolkit's `fork_context_8_to_9` mirrors that, resetting
every wallet's local dust state so it does not try to spend dust the chain no
longer has. Should a translation ever carry dust across again, the replay corrupts
nothing: every replayed event collides with `GenerationInfoAlreadyPresent`, and a
rejected batch leaves the ledger state untouched, so the replay runs to completion
having restored nothing and reporting every page as skipped.

## A full block defers a page, it does not cost one

The price above is per-batch, but the ledger admits a system transaction against the
whole block: `apply_system_tx` checks `tx_cost + block_fullness` against
`block_limits`. Ledger work applied earlier in the same block counts against the same
limit — `pallet_partner_chains_bridge::handle_transfers` is a mandatory inherent
carrying up to 256 C2M transfers, each applying a ledger system transaction, and
inherents run *ahead* of the `inherents_applied()` where migration steps do. So a batch
that priced to fit can still be turned away, and pricing cannot see it coming.

The ledger says which case it is, so the migration asks rather than guesses:
`MidnightSystemTransactionExecutor` gains an `is_block_limit_exceeded` hook, which
`pallet-midnight-system` answers by comparing against its own
`Error::BlockLimitExceededError`. A batch turned away because the block is full is
*deferred* — the step ends with its cursor unmoved, tallying and eventing nothing, and
the next block re-reads that same page against a `block_fullness` that starts at zero.
Only a batch rejected on its own merits is skipped, reported through
`DustReapplyBatchFailed`, and stepped over; with fullness ruled out, such a rejection is
permanent and retrying it would be pointless.

Deferral is deliberately unbounded. Room returns on any quieter block, applying anyway
is a guaranteed rejection, and a stall is visible every block through
`ObservationsSkippedForMigration`. It cannot deadlock either: a batch priced above 90%
of the migration's whole-block budget is given up on before it is ever applied, so
anything that reaches the ledger fits a quiet block.

New events: `DustReapplyStarted`, `DustReapplyBatchFailed`,
`DustReapplyCompleted`, `DustReapplySkipped`, and
`ObservationsSkippedForMigration` — the last one emitted once per block for as
long as the gate above ignores observations, so a multi-minute stall is visible
on chain rather than only in the node log.

## Warp sync during the replay

For the handful of blocks the replay runs, a node that *begins* warp sync targets a block whose
`PreForkStateKey` points at the pre-fork ledger-8 arena root. That root is not part of the warp
snapshot — the snapshot is rooted at `pallet_midnight::StateKey`, i.e. the v9 root, and the
hardfork wiped the dust nodes out of it — so such a node could not read the pre-wipe values, would
cancel the replay locally while every other node applied it, and would diverge on the state root.

The node now detects this and refuses: `warp_ledger_sync`'s recovery monitor checks
`CNightObservation::PreForkStateKey` at the warp target and, if it is set, leaves the recovery gate
armed instead of completing recovery. That gate holds both authoring and block import, so the node
stalls with a logged explanation rather than forking or authoring a block its peers reject. The
refusal is terminal for that database — a restart re-reads the same finalized target — so the node
must be re-synced from a purged database once the replay has finished.

Warp-sync new nodes either before the runtime upgrade or after `DustReapplyCompleted`, not in
between. Targets before the upgrade block are unaffected: their own `StateKey` *is* the ledger-8
root, so the snapshot already carries what the replay later reads.

## Watching it happen

**In a block explorer, `DustReapplyStarted` is the only replay event you will
see.** It is emitted from `on_runtime_upgrade` in the upgrade block, so its phase
is `Initialization`. Everything else the replay produces —
`DustReapplyCompleted` / `DustReapplySkipped` / `DustReapplyBatchFailed`, the
replay's `SystemTransactionApplied`, and `pallet_migrations`' own
`MigrationAdvanced` / `MigrationCompleted` / `UpgradeCompleted` — is deposited
from a multi-block-migration step, which FRAME runs in `inherents_applied()`
after the block's last extrinsic. Their phase is `ApplyExtrinsic(n)` where `n`
is the block's extrinsic count, an index no extrinsic in that block has, and
Polkadot.js Apps renders events grouped under the extrinsic that claims them —
so it drops them silently. They are in `System::Events` either way.

Read them by event rather than by extrinsic: `state_getStorage` on
`System::Events` at the block hash, or subxt's `events()`. Or read the node log —
every one of these events is logged under its own name, so grepping for the event
name finds it whether or not an explorer will show it:

```
DustReapplyStarted: recorded pre-fork ledger state key for the dust generation replay
ObservationsSkippedForMigration: skipping process_tokens (on-chain storage version StorageVersion(1) < StorageVersion(2)); MBM in progress
DustReapplyCompleted: dust generation replay complete, 52 applied, 13 skipped
```

`ObservationsSkippedForMigration` comes from the `process_tokens` inherent, so
that one *is* visible in an explorer, and it brackets the window during which
Cardano observations are being ignored.

The same phase quirk is why the toolkit's block fetcher — which used to collect
system transactions only while walking the extrinsic that matched their phase —
would have dropped every replayed batch, leaving a post-fork replay to fail state
root verification. It now keys on the event's own phase and sorts the block's
transactions into execution order. (The indexer already collected them
unconditionally.)

PR: https://github.com/midnightntwrk/midnight-node/pull/2012
Issues:
- https://github.com/shieldedtech/shielded-security-engineering/issues/548
- https://github.com/shieldedtech/shielded-security-engineering/issues/549
- https://github.com/shieldedtech/shielded-security-engineering/issues/550
