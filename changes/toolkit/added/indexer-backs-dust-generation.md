#toolkit
# Answer `backs_dust_generation` from the indexer

`IndexerContext` now implements `backs_dust_generation`, the check `register-dust-address` uses to
pick a generationless NIGHT UTXO to pay a self-funded fee. It reads the indexer's
`UnshieldedUtxo.registeredForDustGeneration`, which the indexer computes the same way the replay
path does (is the UTXO's initial nonce in `dust.generation.night_indices`). The flag is fixed when
the UTXO is created, so the wallet cache keeps it alongside each UTXO. The cache format version
goes to 5, so existing wallet cache entries (replay and indexer) are rebuilt once.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
