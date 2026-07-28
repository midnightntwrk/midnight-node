#runtime #consensus

# Harden BABE pre-digest checks and consensus-engine transition calls

The consensus-engine pallet now:
- rejects any BABE pre-runtime digest in `State::Aura`;
- rejects any AURA pre-runtime digest in `State::Babe`, so a stray one cannot
  hijack slot or author extraction from a BABE-authored block (`pallet-babe`
  and the BABE verifier ignore foreign digests, and `polkadot-js`
  `extractAuthor` credits the first pre-runtime digest it can decode);
- requires a matching BABE `SecondaryPlain` pre-runtime digest on the
  epoch-boundary flip block, checked only once BABE authorities are populated so
  that blocks produced while waiting for the session rotation still postpone
  instead of being rejected;
- accepts only the `SecondaryPlain` variant as a transition digest — `Primary`
  and `SecondaryVRF` carry VRF material that is never client-verified while
  blocks import through the AURA pipeline;
- rejects malformed BABE digests during `ArmedBabe`/`ScheduledFlip` (wrong slot,
  ordering, or duplicates; absence remains allowed for older binaries);
- returns `Error::InvalidEngineState` from `arm_babe` / `schedule_flip` when
  called from the wrong `EngineState`;
- postpones the flip while `pallet-babe::Authorities` is empty;
- writes `BABE_GENESIS_EPOCH_CONFIG` into `pallet-babe::EpochConfig` at the
  flip when absent (upgraded networks never ran Babe genesis, and
  `Babe::current_epoch()` / `next_epoch()` expect it).

PR: https://github.com/midnightntwrk/midnight-node/pull/1929
Issue: https://github.com/midnightntwrk/midnight-node/issues/1935
