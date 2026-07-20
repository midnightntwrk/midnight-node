#runtime #governance

# Allow the original proposer to cancel a collective proposal early

Adds `pallet-collective-proposer-cancel` and wires it as two instances
(`CouncilProposerCancel`, `TechnicalCommitteeProposerCancel`). Each instance
exposes `cancel_proposal(proposal_hash)` which lets the original proposer
withdraw a `pallet_collective` proposal without waiting out the 5-day voting
window.

Previously, the only way to clear a clearly-bad proposal was to organise
enough NO votes to trigger early disapproval, or to wait for the voting
period to expire — `disapprove_proposal` and `kill` are gated on `EnsureRoot`,
and Root is only reachable through a successful federated motion.

Proposer identity is recovered from `pallet_collective::CostOf`. Both
collectives now use a `RecordProposer` no-deposit `MaybeConsideration` so
that map is populated; under the previous unit `()` Consideration it was
empty.

PR: https://github.com/midnightntwrk/midnight-node/pull/1428
