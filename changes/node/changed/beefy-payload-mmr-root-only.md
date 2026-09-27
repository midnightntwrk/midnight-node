#node #beefy #relay

# BEEFY commitments carry only the MMR root

The BEEFY payload provider is upstream `MmrRootProvider`: each commitment's
payload holds one entry, `mh`, so the signed commitment encodes to the 48 bytes
the Committee Bridge MIP verifies. The `cs`, `cb`, `ns` and `nb` stake entries
and `MmrRootAndBeefyStakesProvder` are removed.

The relay builds its authority proof from the commitment's validator set, over
the same seat leaves as the runtime's MMR leaf commitment
(`midnight_primitives_beefy::authority_set_commitment`), instead of reading
stakes from the payload.
