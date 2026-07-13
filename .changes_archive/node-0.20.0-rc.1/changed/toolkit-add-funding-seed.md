#toolkit
# Add `--funding-seed` option to `single-tx` command

This option allows toolkit users to fund their single-tx transaction using a
different wallet to the unshielded/shielded token source wallet

Useful for performance testing, where the pre-funded wallets contain enough
DUST to fund all transactions on the network.

PR: https://github.com/midnightntwrk/midnight-node/pull/449
Ticket: https://shielded.atlassian.net/browse/PM-21171
