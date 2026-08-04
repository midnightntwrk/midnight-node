#ledger #storage #node
# Garbage-collect ledger_storage to honour state pruning

`ledger_storage` previously retained every persisted arena root forever, so a
pruned validator's ledger DB grew without bound (~197G vs ~7G paritydb on
preprod). A background worker now collects `Midnight::StateKey` values for the
blocks still covered by Substrate `--state-pruning` (plus non-finalized forks)
and runs the storage crate's incremental GC against that live root set.

Archive nodes skip the worker and keep full ledger history.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
