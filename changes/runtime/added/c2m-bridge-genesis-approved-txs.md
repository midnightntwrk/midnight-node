#runtime #c2m-bridge

# Add optional `approved_txs` to c2m-bridge genesis config

The `pallet-c2m-bridge` genesis config gains an optional `approved_txs` field
(`Vec<McTxHash>`). At genesis, every entry is inserted into `ApprovedMcTxHashes`,
pre-approving those mainchain transaction hashes for crediting.

The field is `#[serde(default)]`, so it can be omitted from `chain-spec.json`
(it defaults to empty). When present it is an array of hex strings (e.g.
`"0x0101..."`), not an array of numbers, because `McTxHash` serializes as hex.

PR: <TBD>
Issue: <TBD>
