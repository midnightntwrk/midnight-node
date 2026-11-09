#toolkit #performance
# Cached ledger state lives in an on-disk database

With `--ledger-state-db`, every command used to rebuild the whole cached ledger
state in memory from a snapshot file before doing anything: about 19 s and 9 GB
of memory on preprod at block 600,000. With `PARITYDB_MODE=true` (off by
default) the ledger state is now kept in a ParityDB under
`<ledger-state-db>/ledger-arena`; a snapshot records only the
root of its state, so a command opens it at once and reads only the parts it
uses. Replayed blocks write just the state they changed, unused states are
garbage-collected, and a long replay checkpoints every 10,000 blocks with
bounded memory. A second toolkit process on the same `--ledger-state-db` waits
for the first (`MN_LEDGER_STATE_DB_LOCK_TIMEOUT` bounds the wait). Existing
snapshot files are imported on first use.

PR: TBD
