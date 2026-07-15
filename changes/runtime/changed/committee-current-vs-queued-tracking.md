#runtime #partner-chains
# Track queued committee so CurrentCommittee reflects the active validator set

After the migration to stock `pallet_session`, a validator set handed over at a
session rotation is only applied one session later. `CurrentCommittee` (and the
`get_current_committee` runtime API, the `sidechain_getEpochCommittee` RPC and
everything built on it) was rotated immediately, so for a full session it
reported a committee that was not yet authoring blocks.

`pallet_session_validator_management` now tracks the rotation pipeline in three
stages: `NextCommittee` (selected by the inherent) is moved at rotation to a new
`QueuedCommittee` storage (handed to `pallet_session`, pending application),
and the previously queued committee is promoted to `CurrentCommittee`.
`CurrentCommittee` thereby keeps its original meaning — the committee whose
keys form the effective validator set of the current session — and the
`SessionValidatorManagementApi` is unchanged in shape and semantics:

- `get_current_committee` returns the committee actively producing blocks. At
  promotion the committee's epoch is stamped with its selection epoch + 1 —
  the epoch it was due to start serving — so in normal operation the reported
  epoch matches the epoch the committee is active in (as before the
  `pallet_session` migration). After skipped epochs, catch-up rotations keep
  each recovered committee's label unique and in recovery order instead of
  collapsing them onto the current epoch.
- `get_next_committee` is unchanged: it returns the committee selected by the
  inherent for the upcoming epoch, labeled with its selection epoch (the
  pre-v2 contract). The committee queued in `pallet_session` is internal
  `QueuedCommittee` storage.

Selection bookkeeping (`should_end_session`, the committee-selection inherent,
`get_next_unset_epoch_number`) is anchored on `QueuedCommittee`, preserving the
exact pre-change rotation behavior — no consensus change.

Also repoints the BEEFY stake computation: current stakes match
`pallet_beefy::Authorities` against `CurrentCommittee` (active), next stakes
match `pallet_beefy::NextAuthorities` against `QueuedCommittee` instead of
`NextCommittee`, which is one rotation too far ahead.

Adds pallet storage version 2 with a `V1ToV2Migration` initializing
`QueuedCommittee` from `CurrentCommittee`. Requires a metadata rebuild.
