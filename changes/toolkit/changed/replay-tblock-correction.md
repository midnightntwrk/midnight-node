#toolkit #fix
# Apply the tblock correction when replaying historical ledger-8 blocks

The toolkit couldn't sync Midnight Preview: its ledger replay died at block 112450 with StateRootMismatch. Cause — the node verifies a block's first ledger transaction against parent_block_time + 12s when the runtime imports v1 of Ledger8Bridge::apply_transaction (the #1924 correction). That lives in the host-API layer; the toolkit's replay goes through ledger/helpers and never got it, so a transaction whose DustActions.ctime ran 3s past its block was rejected, silently skipped, and the state root diverged.

This PR ports the correction into the replay: block-start tracking mirroring Ledger::is_block_start, plus the retry-at-uncorrected-tblock fallback.

Verified: full cold replay of 1,468,447 blocks, zero mismatches.

PR: https://github.com/midnightntwrk/midnight-node/pull/2372
Issue: https://github.com/midnightntwrk/midnight-node/issues/2070
