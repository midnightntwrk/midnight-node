# Add Ledger Sync process for Warp Sync

Adds support for syncing ledger state, while warp syncing. Should enable Substrate warp sync.
Non-validator nodes serve ledger snapshots to warp-syncing peers; validators don't by default,
but can opt in via the new `--serve-warp-ledger-sync` flag (off by default). Nodes can warp-sync
as clients regardless of the flag.

PR: https://github.com/midnightntwrk/midnight-node/pull/1650
Issue: https://github.com/midnightntwrk/midnight-node/issues/1648