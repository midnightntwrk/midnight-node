#runtime

# Validate D-Parameter seat counts and keep the committee when a selection is empty

The committee selection now only accepts D-Parameters that request a usable committee size, and
treats an empty selection the same way as a failed one.

- Ariadne V2 adds the seat counts with checked arithmetic and returns `None` if they do not fit.
- The committee pallet keeps the queued committee when the selection is empty, in both
  `create_inherent` and `check_inherent`, and `set` rejects an empty committee with
  `EmptyCommittee`.
- `pallet-system-parameters` requires the D-Parameter's total seat count to be between 1 and the
  new `MaxCommitteeSize` constant (set to `MaxAuthorities`), both in `update_d_parameter`
  (`DParameterEmpty`, `DParameterTooLarge`) and in genesis.

PR: https://github.com/midnightntwrk/midnight-node/pull/2368
