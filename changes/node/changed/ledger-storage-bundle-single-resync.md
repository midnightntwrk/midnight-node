#node #storage #breaking

# Ship the ledger storage changes together so every node resyncs once

The ledger GC (#1991), the indexed and LZ4-compressed ledger columns (#2300) and the removal of
the `separate` storage layout with the switch to upstream parity-db (#2333) each need a database
created from scratch to take effect. They land together, so operators resync each node once.

A database created by this version records storage format 2 in its META column; the node logs the
format at startup and exports `midnight_storage_format{version,status}`, so a resynced node can be
told apart from one still on a format-1 database. Each change has its own change file with the
details.

PR: https://github.com/midnightntwrk/midnight-node/pull/2332
