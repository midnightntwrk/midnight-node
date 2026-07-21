# Fix self-funded dust registration for multi-UTXO wallets

`generate-txs register-dust-address` without `--funding-seed` no longer panics
with `InsufficientDustForRegistrationFee` when the wallet holds more than one
NIGHT UTXO. The ledger only credits retroactive (generationless) DUST from
NIGHT inputs in the intent's guaranteed unshielded offer — which fits a single
input under the guaranteed segment's time-to-dismiss budget — but the builder
requested the DUST sum of *all* UTXOs while placing an arbitrary one in the
guaranteed slot. The dust-richest UTXO is now sorted into the guaranteed spend
and the requested allowance is exactly its accrued DUST, matching what the
ledger makes available for the constructed transaction.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1896
