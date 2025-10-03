# Remove unnecessary proof attempts in toolkit

The toolkit was proving a TX, checking if the fees were paid, and immediately re-proving that TX. Avoid making the second proof.

This should help avoid `BalanceCheckOverspend` errors from the ledger, though we should also be paying a little more than explicitly needed (which will definitely avoid them).

PR: https://github.com/midnightntwrk/midnight-node/pull/64
Ticket: https://shielded.atlassian.net/browse/PM-19755