#runtime #governance #federated-authority-observation #security

# Harden the federated-authority-observation inherent check

`check_inherent` for `reset_members` used to fall through to `Ok(())` when the verifier's inherent
data carried no `FederatedAuthorityData` entry, so a verifier without that entry accepted any
`reset_members` call, i.e. an arbitrary governance membership reset authored by a block producer.
This was only safe because the node's verifier inherent-data providers always supply the entry.

- `check_inherent` now fails with the new fatal `InherentError::InherentNotExpected` when a block
  contains `reset_members` but the inherent data has no federated authority data. The existing
  `CouncilMembersMismatch` / `TechnicalCommitteeMembersMismatch` comparisons are unchanged.
- The pallet now implements `is_inherent_required`, returning the new fatal
  `InherentError::Missing` exactly when `create_inherent` would produce a `reset_members` call. A
  block lacking the inherent is therefore rejected when the observed data is usable, while blocks
  honest authors produce without it (observed members empty, duplicated or over the bound) remain
  valid.

`InherentError` gains two variants ahead of the std-only `Other`; inherent errors are not part of
the runtime metadata, so no metadata rebuild is needed.

PR: TBD
