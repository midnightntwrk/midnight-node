#runtime #node #system-parameters #decentralization

# `cnight_weight` system parameter behind a `decentralization` feature

Adds a `decentralization` cargo feature to the runtime and the node, off by default; the
node's turns on the runtime's. It will carry the permissionless candidates and NIGHT
staking work (NIGHT Staking on Cardano MIP), starting with this change.

With the feature, `pallet-system-parameters` holds `cnight_weight`, the lovelace of virtual
stake per STAR of staked cNIGHT (MIP rule 25), as a fraction `(numerator, denominator)`:

- `update_cnight_weight(numerator, denominator)` sets it, from the same origin as the
  D-Parameter (`SystemOrigin`). A denominator of 0 is refused with
  `CnightWeightZeroDenominator`; the change emits `CnightWeightUpdated`.
- It is always set. Genesis sets it to 1, the MIP's value at activation, itself a
  placeholder (Open Question 6). On an existing chain, the `InitializeCnightWeight`
  migration sets it to 1 and emits `CnightWeightUpdated`, so the first value is on record
  like every later one. The migration checks the storage, not the pallet's storage version,
  which this change leaves alone: once the value exists, it does nothing.

The storage holds only the current value: each NIGHT stake snapshot will record the value
it takes, so a change counts from the next snapshot. The call reuses the D-Parameter
update's weight until it has a benchmark of its own.

Without the feature, the runtime and its metadata are unchanged.

PR: https://github.com/midnightntwrk/midnight-node/pull/2526
Issue: https://github.com/midnightntwrk/midnight-node/issues/2525
