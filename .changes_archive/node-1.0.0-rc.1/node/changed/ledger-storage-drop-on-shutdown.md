#ledger #node
# Drop ledger default storage on node shutdown

Call `midnight_node_ledger::drop_all_default_storage()` after `run_node_until_exit` returns so DB-backed default storages are explicitly released during graceful shutdown.

PR: https://github.com/midnightntwrk/midnight-node/pull/886
Ticket: https://shielded.atlassian.net/browse/PM-22219
