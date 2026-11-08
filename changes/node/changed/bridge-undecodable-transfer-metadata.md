#security #node #bridge
# Read undecodable bridge transfer metadata as an invalid recipient

The Db-Sync token bridge data source read the C-to-M transfer metadatum
(`tx_metadata.json`, key 6500973) straight into `serde_json::Value`. sqlx decodes
`jsonb` with `serde_json::from_slice`, which rejects JSON nested 128 levels or
deeper, so a single metadatum of 128 nested lists failed the whole
`get_bridge_txs` query with `recursion limit exceeded`.

`BridgeTx::c2m_metadata` now decodes through `C2mMetadataIfValid`, which reads
anything it cannot parse as `None`. `metadata_to_recipient` already maps that to
`TransferRecipient::Invalid`, the same as any other unusable metadata, so the
transfer is still observed for its amount and the data checkpoint advances past it.

This mirrors `DbDatumIfValid`, which guards the datum columns against the same
serde_json limit. The metadata column had been left out.

Impact: anyone can pay to the illiquid circulating supply address with metadata of
their choosing, so the failing query was reachable permissionlessly. Because
`get_transfers` feeds the token bridge inherent data provider in both `ProposalCIDP`
and `VerifierCIDP`, the error stopped every node from authoring *and* from verifying,
and the checkpoint could never advance past the offending transaction. The bridge is
inert on all networks while `initialCheckpoint` is null, so this was never live; it
would have become reachable the moment governance called `set_main_chain_scripts`.

Covered by `metadata_nested_too_deeply_to_decode_yields_an_invalid_recipient`, run
against both Db-Sync schema variants and both the cached and uncached data sources.

PR: https://github.com/midnightntwrk/midnight-node/pull/2382
