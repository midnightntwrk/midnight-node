#runtime #consensus

# Harden BABE pre-digest checks for the AURA→BABE transition

The consensus-engine pallet now hard-rejects (assert):
- any BABE pre-runtime digest while still in `State::Aura` (not only the
  canonical AURA-then-matching-BABE shape). Reordered or malformed digests
  previously bypassed the guard while `pallet-babe` still consumed the first
  BABE digest and could deposit an unretractable `NextEpochData` pre-arming;
- an epoch-boundary flip candidate that lacks a matching BABE `SecondaryPlain`
  pre-runtime digest (committing without it would permanently halt the chain);
- any transition-window block (`ArmedBabe`/`ScheduledFlip`) whose BABE digest is
  present but malformed (wrong slot, ordering, or duplicates). Absence remains
  allowed so older binaries can still author until upgraded.

PR:
Issue:
