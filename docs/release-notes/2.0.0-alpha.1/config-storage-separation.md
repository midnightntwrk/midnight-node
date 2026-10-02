<!-- markdownlint-disable MD013 -->
# storage_separation — operator config guide

Controls whether Midnight Ledger storage shares the Substrate ParityDb instance or
runs in its own. Introduced in 2.0.0-alpha.1
([PR #1278](https://github.com/midnightntwrk/midnight-node/pull/1278)).

## What it does

In `separate` mode (the default) the node opens **two** ParityDb instances:

- `<base-path>/chains/<chain-id>/paritydb/` — Substrate storage (blocks, state, etc.)
- `<base-path>/ledger_storage/` — Midnight Ledger storage

Because the two databases commit independently, an unexpected process termination
between the two commits can leave them out of sync, causing a data-integrity error on
next start.

In `unified` mode the node opens **one** ParityDb instance at
`<base-path>/chains/<chain-id>/paritydb/`. Ledger columns are appended to the same
database, so each block's Substrate and Ledger writes land in a single atomic ParityDb
commit — eliminating the cross-database inconsistency window.

## Configuration

There is no CLI flag. Set the value via TOML config or environment variable.

| Method      | Key / variable       | Accepted values             | Default      |
| ----------- | -------------------- | --------------------------- | ------------ |
| TOML        | `storage_separation` | `"separate"`, `"unified"`   | `"separate"` |
| Environment | `STORAGE_SEPARATION` | `separate`, `unified`       | `separate`   |

The key sits at the top level of the config file (same level as `validator`,
`storage_cache_size`, etc.).

**TOML example — opt in to unified:**

```toml
storage_separation = "unified"
```

**Environment variable example:**

```sh
export STORAGE_SEPARATION=unified
./midnight-node --base-path /data ...
```

Values are matched case-insensitively (`"Unified"` and `"UNIFIED"` are also accepted).

## When to use unified

Use `unified` when data integrity after an abrupt node crash is the priority. A single
ParityDb commit is atomic; two separate databases are not. Any unexpected `SIGKILL`,
OOM kill, or power loss while the node is between the two commits produces an
inconsistent state that requires manual recovery.

The local-environment test nodes (nodes 4 and 5) run with `unified` as a reference
configuration. No performance difference has been measured; the column-count increase
is small relative to the total ParityDb workload.

## Switching modes

### `separate` → `unified`

Migrated in place on start-up, since 2.1.0. Both modes store the same
content-addressed ledger nodes; only the database they live in differs. The node
folds `<base-path>/ledger_storage/` into the Substrate ParityDb and carries on —
**no resync**.

1. Stop the node cleanly. The migration reads what ledger storage has flushed to
   disk, so let the process shut down rather than `SIGKILL`ing it.
2. Set `storage_separation = "unified"` (or `STORAGE_SEPARATION=unified`).
3. Start the node. It logs `Migrating ledger storage into the unified database`,
   copies the ledger columns across, and renames the source directory to
   `<base-path>/ledger_storage.migrated/`.
4. Once the node is producing/importing blocks again, delete
   `<base-path>/ledger_storage.migrated/` to reclaim the disk.

The migration needs a single pass over ledger storage, so allow extra start-up
time proportional to its size. It is safe to interrupt: a `ledger_storage.importing`
marker file records an unfinished import and the next start begins it again. Do not
delete `<base-path>/ledger_storage/` while that marker exists — the node refuses to
start rather than run on a partially copied ledger.

If the node instead fails with an `IncompatibleColumnConfig` error, `ledger_storage/`
is missing and there is nothing to migrate from. Restore it, or fall back to the
resync below.

### `unified` databases created before 2.1.0

2.1.0 btree-indexes the ledger columns in `unified` mode; earlier releases left them
unindexed, which would have prevented ledger garbage collection from ever running
against them. Nodes already written without an index cannot be re-indexed in place,
so **a `unified` database from an earlier release will not start on 2.1.0** — it fails
with an `IncompatibleColumnConfig` error. Delete the chain data directory and resync,
or switch back to `separate` before upgrading.

`separate` databases are unaffected: their layout is unchanged.

### `unified` → `separate`

Not supported. Restarting in `separate` mode on a `unified` database fails
immediately with an `IncompatibleColumnConfig` error. To go back:

1. Stop the node cleanly.
2. Delete the chain data directory (`<base-path>/chains/<chain-id>/` **and**
   `<base-path>/ledger_storage/` if it exists).
3. Set `storage_separation = "separate"`.
4. Resync from genesis or from a trusted snapshot.

## References

- [PR #1278](https://github.com/midnightntwrk/midnight-node/pull/1278)
- [Issue #1297](https://github.com/midnightntwrk/midnight-node/issues/1297)
- [Release notes](release-notes-2.0.0-alpha.1.md)
