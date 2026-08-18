#node #consensus #grandpa
# Use shieldedtech polkadot-sdk fork with GRANDPA justification-import race fix

Point all polkadot-sdk dependencies at the `shieldedtech/polkadot-sdk` fork branch
`test-polkadot-stable2606` (stable2606 plus the merged upstream fix
paritytech/polkadot-sdk#12506).

The fix makes `GrandpaBlockImport::import_justification` verify the justification
atomically with finalization under the authority-set lock, removing a
time-of-check/time-of-use race that could panic a validator with
`'returns Ok when no authority set change should be enacted; qed'` when a
commit-message finalization and a block-import justification raced on an
authority-set-change block. This race is triggered in practice by the
`McHashBlockAnnounceValidator` deferring announcements (`Skip`) while local
Cardano observability lags, which builds a commit + block backlog that flushes
at once on recovery.

The fork pin is temporary until a stable2606 patch release (or later stable)
includes the fix.

PR:
