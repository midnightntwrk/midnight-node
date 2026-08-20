#node #consensus #babe #grandpa
# Use shieldedtech polkadot-sdk fork with BABE construction and GRANDPA justification fixes

Point all polkadot-sdk dependencies at the `shieldedtech/polkadot-sdk` fork branch
`test-polkadot-stable2606` (stable2606 plus two merged upstream fixes).

- paritytech/polkadot-sdk#12754: `sc_consensus_babe::prune_finalized` no longer
  panics when the finalized header has no BABE pre-digest. That lets the node
  construct `BabeBlockImport` at startup on an AURA chain (genesis, or any
  finalized AURA block) without a digest-synthesizing client wrapper. Authoring
  is gated only by the AURA→BABE supervisor (no per-slot CIDP engine guard);
  the epoch tree is bootstrapped at the flip for every node role so full nodes
  can import the first BABE block.
- paritytech/polkadot-sdk#12506: `GrandpaBlockImport::import_justification`
  verifies the justification atomically with finalization under the
  authority-set lock, removing a double-finalization race.

The fork pin is temporary until a stable2606 patch release (or later stable)
includes both fixes.

PR:
Issue:
