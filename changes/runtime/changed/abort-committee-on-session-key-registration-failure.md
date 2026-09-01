#runtime #partner-chains
# Abort committee rotation when session key registration fails

`pallet_session_validator_management` no longer treats `SessionInterface::set_keys`
as best-effort. `set_keys` is a black box: any error from `pallet_session` aborts
the hand-off. `new_session` returns `None` so `pallet_session` keeps the previous
authority set. A reduced committee is never installed.

The failed committee is consumed and the queued epoch is advanced so session
rotation does not retry every block. The previously queued committee stays as
both the current and queued set, matching the authorities `pallet_session`
retains. `set_keys` writes run in a storage layer and are rolled back on
failure, so a retained validator cannot keep rotated keys while committee
storage still records the previous ones. Genesis panics if the initial
committee cannot be registered in full.

This keeps `CurrentCommittee`/`QueuedCommittee` aligned with the live session
validator set, so AURA author-index and BEEFY stake matching stay defined.

PR: https://github.com/midnightntwrk/midnight-node/pull/2078
Issue: https://github.com/midnightntwrk/midnight-node/issues/1895
