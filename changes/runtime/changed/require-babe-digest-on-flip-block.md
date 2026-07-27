#runtime #consensus

# Require matching BABE pre-digest on the AURA→BABE flip (and during transition)

The consensus-engine pallet now hard-rejects (assert):
- an epoch-boundary flip candidate that lacks a matching BABE `SecondaryPlain`
  pre-runtime digest (committing without it would permanently halt the chain);
- any transition-window block (`ArmedBabe`/`ScheduledFlip`) whose BABE digest is
  present but malformed (wrong slot, ordering, or duplicates). Absence remains
  allowed so older binaries can still author until upgraded.

PR:
Issue:
