#hardfork #ledger
# Use the upstream ledger v8->v9 state translation crate

Replaces the vendored copy of the v8->v9 ledger state translation table with a git
dependency on the ledger team's `v8-to-v9-state-translation` crate, so ledger-side fixes
land without a manual re-port and the node stays aligned with downstream consumers
(indexer).

No behaviour change: the vendored copy's manual dust wipe is now upstream (midnight-ledger
PR #707), which closes the `TODO` the vendored file carried.

PR: https://github.com/midnightntwrk/midnight-node/pull/2054
Issue: https://github.com/midnightntwrk/midnight-node/issues/2049
