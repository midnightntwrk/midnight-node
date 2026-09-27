#runtime #beefy #mmr

# Commit the BEEFY authority sets with seat counts

The MMR leaf's authority sets are the Committee Bridge MIP's committee
commitment: one `key ‖ seats (u32 LE)` leaf per distinct key, ascending by key,
under a Keccak binary Merkle root, with `len` the total seat count. A key that
holds several committee seats appears once, with its seat count, instead of once
per seat. `SeatCommitments` replaces `BeefyMmrLeaf` as
`pallet_beefy::Config::OnNewValidatorSet`; the leaf format and version (0) are
unchanged.

`midnight_primitives_beefy::authority_set_commitment` computes it, so the relay
and the runtime share one definition; its test checks the contract's vector.
