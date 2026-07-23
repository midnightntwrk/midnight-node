#node

# Emit BABE SecondaryPlain pre-runtime digest while the flip to BABE is armed

Block authors now attach a BABE `SecondaryPlain` pre-runtime digest to the
blocks they produce once the consensus-engine flip to BABE is armed
(`ArmedBabe`/`ScheduledFlip`), while blocks are still authored with AURA. The
digest carries the block's AURA slot and `authority_index = slot % n_authorities`
(the same index the AURA logic uses), so the network is already running BABE
digests before governance schedules and commits the flip.

- Adds `ConsensusEngineApi` method `should_emit_babe_preruntime_digest()`
  without bumping API version because this isn't release/deployed API yet.
- Adds `ArmedBabeProposerFactory`, a proposer wrapper that reads the arming
  state and AURA authority count at the parent and appends the pre-digest.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1751
