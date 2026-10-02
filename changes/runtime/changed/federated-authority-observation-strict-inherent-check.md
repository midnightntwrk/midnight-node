#runtime #governance #federated-authority-observation #security

# Harden the federated-authority-observation inherent check

`check_inherent` for `reset_members` used to fall through to `Ok(())` when the verifier's inherent
data carried no `FederatedAuthorityData` entry, so a verifier without that entry accepted any
`reset_members` call, i.e. an arbitrary governance membership reset authored by a block producer.
This was only safe because the node's verifier inherent-data providers always supply the entry.

- `check_inherent` accepts a `reset_members` call only when it is exactly the call
  `create_inherent` would produce. Otherwise it fails with the new fatal
  `InherentError::InherentNotExpected` (data absent, empty, duplicated, over the bound, or
  undecodable) or with the existing `CouncilMembersMismatch` /
  `TechnicalCommitteeMembersMismatch` errors when the observed members differ.
- `reset_members` now returns `EmptyMembers` or `DuplicatedMembers` before any storage write.
  It previously logged duplicates and still applied them, and treated an empty set as a
  successful no-op. `create_inherent` already skips both, so honest blocks never include a
  call that fails here. Execution rejects the block even if `check_inherent` does not run.
- The pallet now implements `is_inherent_required`, returning the new fatal
  `InherentError::Missing` exactly when `create_inherent` would produce a `reset_members` call. A
  block lacking the inherent is therefore rejected when the observed data is usable, while blocks
  honest authors produce without it (observed members empty, duplicated or over the bound) remain
  valid.

`InherentError` gains two variants ahead of the std-only `Other`; inherent errors are not part of
the runtime metadata, so no metadata rebuild is needed.

PR: TBD
