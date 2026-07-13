#toolkit
# Add `--dust-grace-period` option to ledger parameter commands

Adds the ability to configure `LedgerParameters::dust.dust_grace_period` via the
`show-ledger-parameters` and `update-ledger-parameters` toolkit commands.

This allows users to customize the dust grace period (default 3 hours) so that
a batch of transactions can be reused on new test runs with fresh chains.

PR: https://github.com/midnightntwrk/midnight-node/pull/464
Ticket: https://shielded.atlassian.net/browse/PM-21144
