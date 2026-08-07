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
  through a new `dust_generation_values_v8` host function, and re-applies them in
  batches of 200 as `CNightGeneratesDustUpdate` system transactions;
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
Only the accrual clock resets: the replay stamps the fork block's time as the
creation time, because the original lives on the dust UTXO the wipe takes, and
replaying an older one would re-credit accrual holders have already claimed.
Accrued-but-unclaimed DUST is lost, which is intrinsic to a dust-state wipe.

**Only cnight's slice of the generating set is restored.** Native NIGHT registers
generation entries too (at dust registration, for delegated unshielded outputs,
and on the mint/claim path); nothing in this repo records which of those the wipe
took, so `DustReapplyCompleted` must not be read as "all dust generation
restored".

Until the ledger update that wipes dust ships, the migration is inert: the
current state translation carries dust across, so the first replayed event
collides with `GenerationInfoAlreadyPresent` and the migration self-cancels with
a `DustReapplySkipped` event. It activates on its own when that ledger change
lands.

New events: `DustReapplyStarted`, `DustReapplyBatchFailed`,
`DustReapplyCompleted`, `DustReapplySkipped`.

PR: <link to PR>
