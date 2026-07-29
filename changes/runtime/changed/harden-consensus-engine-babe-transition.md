#runtime #consensus

# Harden BABE pre-digest checks and consensus-engine transition calls

The consensus-engine pallet now:
- rejects any BABE pre-runtime digest in `State::Aura`;
- rejects any AURA pre-runtime digest in `State::Babe`, so a stray one cannot
  hijack slot or author extraction from a BABE-authored block (`pallet-babe`
  and the BABE verifier ignore foreign digests, and `polkadot-js`
  `extractAuthor` credits the first pre-runtime digest it can decode);
- requires a matching BABE `SecondaryPlain` pre-runtime digest on every
  `ArmedBabe`/`ScheduledFlip` block — a block missing it, or carrying a
  malformed one (wrong slot, ordering, duplicates, or a `Primary`/`SecondaryVRF`
  variant, whose VRF material is never client-verified while blocks import
  through the AURA pipeline), is rejected on import, so nodes must be updated to
  emit the digest before governance arms the flip;
- keys all of these digest checks on the pre-runtime item's engine id rather than
  on a successful decode. `DigestItem::as_babe_pre_digest`/`as_aura_pre_digest`
  decode with `DecodeAll` and yield `None` both for undecodable payloads and for
  valid payloads with trailing bytes, while `pallet-babe`/`pallet-aura` read the
  same items with plain `Decode` and consume them — so such items used to slip
  past the guards while still taking effect on-chain, and are now rejected;
- returns `Error::InvalidEngineState` from `arm_babe` / `schedule_flip` when
  called from the wrong `EngineState`;
- postpones the flip while `pallet-babe::Authorities` is empty;
- writes `BABE_GENESIS_EPOCH_CONFIG` into `pallet-babe::EpochConfig` at the
  flip when absent (upgraded networks never ran Babe genesis, and
  `Babe::current_epoch()` / `next_epoch()` expect it).

PR: https://github.com/midnightntwrk/midnight-node/pull/1929
Issue: https://github.com/midnightntwrk/midnight-node/issues/1935
