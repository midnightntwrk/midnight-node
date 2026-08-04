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

**Switching between `separate` and `unified` on an existing database is not
supported.** The column layout written into ParityDb metadata at first open is
incompatible between the two modes. Attempting to restart with a different value will
fail immediately with an `IncompatibleColumnConfig` error that includes the hint:

> Switching between `separate` and `unified` is not supported on an existing
> database — to change `storage_separation`, delete the chain data directory and resync.

To switch:

1. Stop the node cleanly.
2. Delete the chain data directory (`<base-path>/chains/<chain-id>/` **and**
   `<base-path>/ledger_storage/` if it exists).
3. Set the new `storage_separation` value.
4. Resync from genesis or from a trusted snapshot.

Pick the mode before first start on a new node; do not rely on being able to change it
later.

## References

- [PR #1278](https://github.com/midnightntwrk/midnight-node/pull/1278)
- [Issue #1297](https://github.com/midnightntwrk/midnight-node/issues/1297)
- [Release notes](release-notes-2.0.0-alpha.1.md)
