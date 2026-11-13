#runtime

# Remove `pallet_authorship` from the runtime

Drops `pallet_authorship` (index 9) again. Nothing consumes the credited author yet, and the
stock `FindAccountFromAuthorIndex` lookup is wrong on every BABE block that opens an epoch:
BABE signs that block with the set `pallet_session` activates in the same block, but authorship
runs before the rotation and maps the signer's index into the outgoing validators. Carrying a
rotation-aware lookup through the AURA to BABE transition is not worth it for a pallet without a
consumer. Block production rewards (MIP-0024) will reintroduce an author lookup after the flip,
where declaring the pallet after `Session` makes the stock lookup correct.

The pallet keeps no persistent storage (the author is cleared at block finalization), so no
migration is needed. Metadata is regenerated.

PR: https://github.com/midnightntwrk/midnight-node/pull/2310
Issue: https://github.com/midnightntwrk/midnight-node/issues/1875
