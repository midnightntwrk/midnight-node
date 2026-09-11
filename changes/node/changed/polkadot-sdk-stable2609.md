#node #runtime #consensus #babe #grandpa

# Move polkadot-sdk back to upstream paritytech branch stable2609

THIS IS WIP. It should be a change to polkadotsdk-stable2609. Do not merge without fixing it and editing this file

Points all polkadot-sdk dependencies at `paritytech/polkadot-sdk` branch `stable2609`
instead of the temporary `shieldedtech/polkadot-sdk` fork branch `test-polkadot-stable2606`.

The fork only carried two upstream fixes on top of stable2606, both of which are
included in stable2609:

- paritytech/polkadot-sdk#12754: `sc_consensus_babe::prune_finalized` skips epoch
  pruning when the finalized header has no BABE pre-digest (lets `BabeBlockImport`
  be constructed on an AURA chain).
- paritytech/polkadot-sdk#12506: `GrandpaBlockImport::import_justification` verifies
  the justification atomically with finalization, removing a double-finalization race.

The `shieldedtech/polkadot-sdk` source is removed from `deny.toml`'s git allow-list.

PR: https://github.com/midnightntwrk/midnight-node/pull/2113
Issue: https://github.com/midnightntwrk/midnight-node/issues/1757
