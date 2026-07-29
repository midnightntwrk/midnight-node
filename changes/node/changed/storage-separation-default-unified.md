#storage
# `storage_separation` now defaults to `unified`

`res/cfg/default.toml` sets `storage_separation = "unified"`, so every network preset that does not override it now keeps Midnight Ledger and Substrate storage items in a single ParityDb instance instead of two. This is the mode that avoids cross-instance data-integrity errors when the node process is terminated unexpectedly.

Existing nodes do not need a resync. The first start-up after the upgrade folds `<base-path>/ledger_storage/` into the shared ParityDb in place and renames the source to `<base-path>/ledger_storage.migrated/` — expect one extra pass over ledger storage on that start, and keep the migrated directory until the node is healthy. A node that was already running `unified` from a build before the ledger columns were btree-indexed is the one case that still needs a resync.

Operators who want the previous behaviour can set `storage_separation = "separate"` explicitly (TOML key or `STORAGE_SEPARATION=separate`). Doing so on a database that has already been migrated to `unified` requires a resync — `unified` -> `separate` is not supported.

PR: https://github.com/midnightntwrk/midnight-node/pull/1948
Issue: https://github.com/midnightntwrk/midnight-node/issues/1464
