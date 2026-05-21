#runtime
# Add per-account transaction count limit to throttle pallet

Extends the existing per-account throttle to also enforce a maximum number of transactions (`MaxTxs`) within each rolling block window, alongside the existing byte limit. The `AccountUsage` storage is migrated from a 2-field tuple to a `UsageStats` struct (adding `txs_used`). Includes a storage migration that clears the old map on upgrade.

PR: https://github.com/midnightntwrk/midnight-node/pull/1060
Ticket: https://shielded.atlassian.net/browse/PM-22377
