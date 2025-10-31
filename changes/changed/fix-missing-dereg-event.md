#cnight-generates-dust
#runtime
# Fix missing `Deregistration` event when user adds second mapping

When a user adds a second mapping on Cardano, this invalidates their existing registration. Therefore, we should emit a deregistration event.

Ticket: https://shielded.atlassian.net/browse/PM-20229
PR: https://github.com/midnightntwrk/midnight-node/pull/189
