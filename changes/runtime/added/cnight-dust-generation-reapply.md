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
`DustReapplyCompleted`, `DustReapplySkipped`.

PR: <link to PR>
