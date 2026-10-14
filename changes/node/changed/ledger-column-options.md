#node #storage

# Apply storage-core's options to the ledger columns in unified storage

The node's parity-db opener now applies storage-core's ledger column options in both
`storage_separation` modes. Before, it applied them only in separate mode, where the
ledger columns of the main database are unused, so in unified mode the ledger columns
had plain parity-db defaults: the root and node columns were not btree-indexed, which
storage-core needs to iterate them (`get_roots`, `size`, the GC scan), and the node
column was stored uncompressed. A fresh unified database now gets the index and LZ4 on
the node column.

Column options are fixed when a database is created, so an existing database keeps the
ledger column options it was created with and opens as before; only newly created
databases change.

With both modes on the same ledger column options, parity-db no longer rejects reopening
a data directory in the other mode. The mode is now recorded in the META column on first
open, and opening a database in the other mode than it was created in fails with a
`storage_separation` error, as it did before. For a database that predates the record,
the presence of the separate-mode `ledger_storage/` directory decides.

PR: TBD
Issue: https://github.com/midnightntwrk/midnight-node/issues/2294
