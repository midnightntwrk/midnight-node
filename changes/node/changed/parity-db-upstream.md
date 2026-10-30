#node #storage #dependencies

# Use upstream parity-db instead of the midnightntwrk fork

Drops the `[patch.crates-io]` override that pointed `parity-db` at the `midnightntwrk/parity-db`
fork, so the node builds against upstream parity-db 0.5.7 from crates.io. The fork's only change
was lowering `MIN_LOG_SIZE_BYTES` from 64 MiB to 16 MiB, a flush threshold tuned for the
`separate` storage layout, where the ledger had its own parity-db. With the node running `unified`
only, the upstream default applies.

PR: https://github.com/midnightntwrk/midnight-node/pull/2333
Issue: https://github.com/midnightntwrk/midnight-node/issues/1464
