#toolkit
# Fix DustWallet spend state propagation

Fix `DustWallet::speculative_spend` to return the updated `DustLocalState`
alongside spends, and extend `mark_spent` to commit the state atomically
with nullifier recording. This ensures `DustLocalState::spend`'s
`pending_until` flags are propagated, preventing `utxos()` from returning
already-spent outputs in consecutive spend operations.

Addresses Least Authority audit finding Issue AO.

PR: https://github.com/midnightntwrk/midnight-node/pull/877
JIRA: https://shielded.atlassian.net/browse/PM-20016
