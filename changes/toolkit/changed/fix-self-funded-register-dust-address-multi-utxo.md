#toolkit #bugfix
# Fix self-funded register-dust-address for wallets with multiple NIGHT UTXOs

Self-funded `generate-txs register-dust-address` (no `--funding-seed`) panicked with
`InsufficientDustForRegistrationFee` when the wallet held more than one NIGHT UTXO: the
builder requested a fee allowance summed over all UTXOs, but the ledger only grants the
retroactive DUST of generationless NIGHT spent in the guaranteed unshielded offer, which
holds a single UTXO (more would exceed the time-to-dismiss limit).

The builder now puts the generationless UTXO with the most retroactive DUST into the
guaranteed offer and requests exactly its availability. UTXOs already backing DUST
generation are excluded via the new `BuilderContext::backs_dust_generation`. Balancing
failures return an actionable error instead of panicking.

PR: https://github.com/midnightntwrk/midnight-node/pull/XXXX
Issue: https://github.com/midnightntwrk/midnight-node/issues/1896
