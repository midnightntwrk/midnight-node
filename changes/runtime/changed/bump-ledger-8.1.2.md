#runtime #ledger #security
# Bump ledger 8 to 8.1.2 (onyx-coyote security release)

Moves the ledger 8 pin from 8.1.1 to the
8.1.2 release

- hardening of low-level deserialization across `serialize`, `base-crypto`,
  `storage`, `onchain-state`, `onchain-vm` and `transient-crypto` — non-canonical
  encodings and values violating their type's invariant are now rejected rather
  than decoded, so an 8.1.2 ledger rejects data an 8.1.1 ledger accepts
- `DustParameters::time_to_cap` guards against a zero `generation_decay_rate`
  instead of dividing by zero
- saturating Dust `seq` increments and saturating delta accumulation in
  `normalize_deltas`
- Zswap binding randomness extraction no longer panics on a proof preimage with
  no witness to extract from
- contract call cost accounting counts public inputs via
  `ContractCall::public_inputs_len` with saturating arithmetic instead of
  materializing the inputs

Crate versions: `midnight-ledger`/`midnight-zswap` 8.1.2, `midnight-storage`
2.0.3, `midnight-storage-core` 1.2.1, `midnight-onchain-runtime` 3.1.1,
`midnight-onchain-state` 3.0.1, `midnight-onchain-vm` 3.1.1,
`midnight-base-crypto` 1.0.1, `midnight-transient-crypto` 2.1.1,
`midnight-coin-structure` 2.0.2, `midnight-zkir` 2.1.1, `midnight-serialize`
1.1.1. The ledger 7 crates remain on their published crates.io versions.

PR: https://github.com/midnightntwrk/midnight-node/pull/2145/
