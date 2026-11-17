#runtime #governance
# Inherents can no longer be paused through `TxPause`

`TxPause::pause` now rejects the runtime's inherents (`Timestamp::set`,
`SessionCommitteeManagement::set`, `CNightObservation::process_tokens`,
`Bridge::handle_transfers`, `FederatedAuthorityObservation::reset_members`) with
`Unpausable`.

Inherents are required in every block, and `TxPause` is part of `BaseCallFilter`, which also
applies to the None origin inherents dispatch with. The whitelist is `InherentCalls`, the type
safe mode already uses, and a test checks both of its lists against the runtime's inherents.

PR: https://github.com/midnightntwrk/midnight-node/pull/2371
