#ledger
# Remove block reward coin minting functionality

Removed the `mint_coins` functionality from the ledger bridge API and the midnight pallet's block reward minting logic.
This removes the ability to mint coins as block rewards through the ledger's mint_coins API.
This will later be reworked as no NIGHT is being minted when block rewards are paid out.

Changes include:

- Removed `mint_coins` method from `LedgerBridge` trait
- Removed block reward minting logic from midnight pallet's `on_finalize` hook

PR: https://github.com/midnightntwrk/midnight-node/pull/451
JIRA: https://shielded.atlassian.net/browse/PM-21159
