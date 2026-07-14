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
  promotion the committee's epoch is stamped with the epoch it starts serving
  in, so the reported epoch matches the epoch the committee is active in (as
  before the `pallet_session` migration), not the epoch it was selected for.
- `get_next_committee` returns the committee that becomes active at the next
  rotation (the queued one), labeled with the epoch it was selected for.

Selection bookkeeping (`should_end_session`, the committee-selection inherent,
`get_next_unset_epoch_number`) is anchored on `QueuedCommittee`, preserving the
exact pre-change rotation behavior — no consensus change.

Also repoints the BEEFY stake computation: current stakes match
`pallet_beefy::Authorities` against `CurrentCommittee` (active), next stakes
match `pallet_beefy::NextAuthorities` against `QueuedCommittee` instead of
`NextCommittee`, which is one rotation too far ahead.

Adds pallet storage version 2 with a `V1ToV2Migration` initializing
`QueuedCommittee` from `CurrentCommittee`. Requires a metadata rebuild.
