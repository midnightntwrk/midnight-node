#toolkit
# Lock redb fetch cache against concurrent toolkit processes

Two concurrent toolkit invocations sharing the same redb fetch cache could
corrupt it, sending one terminal back to genesis and causing the other's
transaction to fail with `Invalid Transaction (1010)`.

`RedbBackend` now takes an exclusive advisory lock on a `<path>.lock` sidecar
before opening redb. If another toolkit process holds the lock, the second one
prints `waiting for lock on redb cache ...` and blocks until it is released.

PR: https://github.com/midnightntwrk/midnight-node/pull/1493
Issue: https://github.com/midnightntwrk/midnight-node/issues/1401
