#storage
# Switching `storage_separation` to `unified` no longer needs a resync

Setting `storage_separation = "unified"` on an existing node used to fail to start with an `IncompatibleColumnConfig` error, leaving operators to delete their chain data and resync from genesis. That was never necessary: both modes hold the same content-addressed ledger nodes, just in different ParityDb instances.

The node now folds `<base-path>/ledger_storage/` into the shared ParityDb on start-up, then renames it to `<base-path>/ledger_storage.migrated/` so it can be deleted once the node is healthy. Expect start-up to take a single extra pass over ledger storage. An interrupted migration is recorded in a `ledger_storage.importing` marker file and resumes on the next start; the node refuses to start if that marker is present but the source database has been removed.

`unified` -> `separate` is still unsupported and still requires a resync.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
