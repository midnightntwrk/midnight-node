#security #runtime #governance
# Require Root to reset the governance membership bodies

`CouncilMembership` and `TechnicalCommitteeMembership` accepted `reset_members` from
the `None` origin, so the federated authority observation inherent could replace
both bodies. `EnsureNone` only proves the call arrived as a bare extrinsic, not that
the inherent sent it. `pallet_membership` has no `ValidateUnsigned`, and the
runtime's generated `pre_dispatch` lets such calls through during block execution.
`check_inherents` also ignores calls no pallet claims as inherents.

Impact: a single block author could append bare `reset_members` calls that make an
account they control the only member of both bodies. That account could then pass a
`FederatedAuthority` motion and dispatch arbitrary calls as Root in the same block.
Existing members did not need to approve.

`ResetOrigin` is now `EnsureRoot` for both instances. `MembershipObservationHandler`
dispatches the validated replacement as Root. The observation inherent still updates
both bodies, and a bare `reset_members` now fails with `BadOrigin`.

Covered by `bare_membership_reset_is_rejected` and
`observed_membership_reset_updates_both_bodies` in the runtime tests.

PR: https://github.com/midnightntwrk/midnight-node/pull/2382
