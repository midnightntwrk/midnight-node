#node #storage
# Record the database storage format in the META column

A node that creates a fresh main database now writes two META-column keys:
`midnight:storage_format` (ASCII decimal, `2` for the complete bundle: unified layout, btree-indexed
and LZ4-compressed ledger columns, tagged ledger roots from genesis) and
`midnight:storage_created_by` (the version of the creating node, as printed by `--version`). The
keys are written only on fresh creation and never stamped onto an existing database; an absent
`midnight:storage_format` means format 1 (legacy). An unparsable value is treated as legacy, and a
value above 2 is accepted with a warning that a newer node created the database.

At startup the node logs one INFO line, `Storage format: 2 (current), created by node <version>` or
`Storage format: 1 (legacy): ...; resync to enable ledger GC and compressed ledger columns`, and
exposes the Prometheus gauge `midnight_storage_format{version,status}`, whose matching series is 1.

The marker replaces `midnight:ledger_gc_tagged_roots` as the ledger GC gate.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1464
