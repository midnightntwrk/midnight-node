#runtime #governance
# Update ubuntu to trixie

Before, the Federated Authority members were only updated when the Substrate part changed. This was forcing an update of the Substrate keys even when only a Cardano key needed to be updated.

Now the Federated Authority observation is more flexible, allowing both key types to be updated independently.

PR: https://github.com/midnightntwrk/midnight-node/pull/252
Ticket: https://shielded.atlassian.net/browse/PM-20485
