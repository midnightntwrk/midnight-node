#node #storage #breaking

# Remove the `separate` ledger storage layout; the node runs `unified` only

The ledger arena now always lives in extra columns of the single Substrate parity-db (the layout
previously selected with `storage_separation = "unified"`). The `separate` layout, which kept the
ledger in its own parity-db under `<base-path>/ledger_storage/`, is gone: there is no storage mode
switch at open, no per-mode dispatch in the ledger host functions, and `res/cfg/default.toml` no
longer sets `storage_separation`.

**Breaking for operators still on the `separate` layout** (the previous default). The node cannot
migrate such data in place and refuses to start on it instead of silently seeding an empty ledger
next to an existing chain: a data directory that has `<base-path>/ledger_storage/`, or whose
Substrate parity-db has no ledger columns, or whose META-column storage-mode marker reads
`separate`, or that has no marker but ledger columns carrying the separate-mode column options,
fails at startup with an error naming the paths to remove. To upgrade, stop the node,
remove the chain data directory and `ledger_storage/`, and resync from genesis or from a
unified-layout snapshot. A node already on `unified` keeps its database as is.

`storage_separation` (and `STORAGE_SEPARATION`) is deprecated: `unified` is still accepted as a
no-op so existing configs keep working, `separate` is rejected at config validation with the same
resync guidance.

Whether a database is on the new storage format is recorded by the `midnight:storage_format` marker
(see `changes/node/added/storage-format-marker.md`).

PR: https://github.com/midnightntwrk/midnight-node/pull/2333
Issue: https://github.com/midnightntwrk/midnight-node/issues/1464
