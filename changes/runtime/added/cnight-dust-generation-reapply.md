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
  state's dust `time_to_cap`), and re-applies them in batches of 200 as
  `CNightGeneratesDustUpdate` system transactions;
- incoming Cardano observations are ignored while it runs — the existing storage
  version gate in `process_tokens` covers it, so `NextCardanoPosition` does not
  advance and the observer re-delivers the same UTXOs afterwards.

It is paced at one batch per block on purpose. `process_tokens`' benchmark
observes registration UTXOs, which never reach the ledger, so its weight says
nothing about the cost of 200 dust `Create`s; left to the weight meter the MBM
service budget would service ~100 batches — 20k ledger creates — in one block.
Measured live sets on 2026-08-06: **mainnet 4870** nonces (finalized #2019697),
**preview 1524** (#301994), **preprod 85** (#1985972) — so ~25 blocks (~2.5 min)
of gated observation on mainnet, ~8 on preview, 1 on preprod.

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
longer has. Should a translation ever carry dust across again, the migration
self-cancels rather than corrupting state: the first replayed event collides with
`GenerationInfoAlreadyPresent` and it emits `DustReapplySkipped`.

New events: `DustReapplyStarted`, `DustReapplyBatchFailed`,
`DustReapplyCompleted`, `DustReapplySkipped`, and
`ObservationsSkippedForMigration` — the last one emitted once per block for as
long as the gate above ignores observations, so a multi-minute stall is visible
on chain rather than only in the node log.

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
