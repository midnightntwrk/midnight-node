#runtime #toolkit

# Fix overfilled blocks report max fullness instead of 0

Overfilled blocks now report 100% fullness instead of 0%, fixing fee adjustment calculations.

**Toolkit:** If historical overfilled blocks exist, the toolkit's replayed ledger state will diverge from on-chain state for blocks before the runtime upgrade.

PR: https://github.com/midnightntwrk/midnight-node/pull/559
Ticket: https://shielded.atlassian.net/browse/PM-20839