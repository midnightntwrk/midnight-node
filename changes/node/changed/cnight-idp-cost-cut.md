#node #cnight #security

# cNIGHT observation inherent is cut by pessimistic cost

The inherent data provider now asks the runtime for a cost model
(`get_observation_cost_model`, API version 2): the pallet weight of processing one
observed UTXO, and the ledger prices of a `Create` and a `Destroy` dust event. It
admits UTXOs only while the pessimistic bound (every UTXO its weight, every
`AssetCreate` additionally a `Create`, every `AssetSpend` a `Destroy`) fits the
runtime's observation budget. Registrations and deregistrations are bounded too, so
no flood of them can reach the pallet.

The cut may fall inside a Cardano transaction: the provider reads the runtime's
`get_next_cardano_cursor` and resumes after the UTXOs of that transaction
already observed, so a transaction too large for one block (~320 cNIGHT spends;
mainnet has carried 427) drains over several instead of stalling observation for
good. The order those UTXOs are counted in is now part of the API.

Under a cost model no count bound applies and the fetch row cap is derived from the
model (`full_gas / utxo_gas + 1` rows per UTXO kind, plus the offset). A kind whose
raw rows fill the cap sets a frontier the batch does not pass, so rows the cap cut
off, or rows that do not decode, can never let the cursor skip past unfetched rows.

If a version 2 runtime cannot price a batch (no cost model, or a zero per-UTXO
weight), the provider halts observation instead of falling back to the count bound,
which can skip rows: it logs an error and submits an empty batch at the current
cursor, so blocks keep being produced and nothing is lost until a runtime upgrade
restores pricing.

Against a runtime without the version 2 API the provider builds exactly the
inherent it built before, byte for byte, so this binary can be rolled out
gradually and mixed with older binaries until the runtime upgrade. After the
upgrade, older binaries build a different inherent and must not remain in the
validator set.

PR: https://github.com/midnightntwrk/midnight-node/pull/2382
