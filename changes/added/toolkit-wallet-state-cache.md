#toolkit
# Add wallet state caching to eliminate genesis replay

Implement a wallet state caching mechanism to persist LedgerContext and Wallet state
across toolkit sessions. Subsequent sessions restore cached state and only replay new
blocks since the checkpoint, dramatically improving startup time on long-running networks.

PR: TBD
Ticket: https://shielded.atlassian.net/browse/PM-21139
