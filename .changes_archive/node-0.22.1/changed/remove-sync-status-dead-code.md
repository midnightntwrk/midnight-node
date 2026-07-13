#node

# Remove unused SyncStatusExt and sync-status-monitor

Remove the SyncStatusExt runtime extension, the sync-status-monitor background task, and associated is_syncing plumbing. This code was never read by any consumer — UTXO ordering for historical blocks is handled by the per-transaction override data files instead.

PR: https://github.com/midnightntwrk/midnight-node/pull/811
