#runtime #consensus

# Harden BABE pre-digest checks and consensus-engine transition calls

The consensus-engine pallet now:
- rejects any BABE pre-runtime digest in `State::Aura`;
- requires a matching BABE `SecondaryPlain` pre-runtime digest on the
  epoch-boundary flip block;
- rejects malformed BABE digests during `ArmedBabe`/`ScheduledFlip` (wrong slot,
  ordering, or duplicates; absence remains allowed for older binaries);
- returns `Error::InvalidEngineState` from `arm_babe` / `schedule_flip` when
  called from the wrong `EngineState`;
- postpones the flip while `pallet-babe::Authorities` is empty;
- writes `BABE_GENESIS_EPOCH_CONFIG` into `pallet-babe::EpochConfig` at the
  flip when absent (upgraded networks never ran Babe genesis, and
  `Babe::current_epoch()` / `next_epoch()` expect it).

PR:
Issue:
