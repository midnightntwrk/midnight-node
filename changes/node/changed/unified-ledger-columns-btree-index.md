#storage
# `unified` storage now btree-indexes its ledger columns

In `storage_separation = "unified"` the Midnight Ledger columns of the shared ParityDb were left at ParityDb's defaults, with no btree index — while `separate` mode applied the ledger's own column options to the same columns, which it reserves but never writes to. The flag was on the wrong mode.

`midnight-storage`'s `get_roots`, `size` and `scan` all iterate those columns, and ParityDb can only iterate an indexed column, so ledger garbage collection could not have run against a `unified` database. Nothing in the node calls it today, so this was latent rather than an active fault. Both modes now index all three ledger columns.

**This changes the on-disk column layout of `unified` databases.** A `unified` database created before this release will refuse to start, because nodes already written outside a btree cannot be re-indexed in place — delete the chain data directory and resync, or switch back to `separate` before upgrading. Deployed nodes run in `separate` mode, whose layout is unchanged and which continues to start normally. Local-environment nodes 4 and 5 run `unified` on volumes that are recreated per run.

Ledger nodes remain uncompressed in `unified` mode, matching how the standalone ledger database stores them, so migrated bytes are identical either side of a `separate` -> `unified` switch. Compressing them would be a separate change needing its own migration.

PR: https://github.com/midnightntwrk/midnight-node/pull/1948
Issue: https://github.com/midnightntwrk/midnight-node/issues/1464
