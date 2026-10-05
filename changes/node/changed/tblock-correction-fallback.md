#node
# Retry the tblock correction's first tx at the block's own timestamp

The v1 tblock correction verifies a historical block's first ledger transaction at
`parent_block_time + 12s` only, on the assumption that the producer always verified it there.
Mainnet #1788980 disproves this: its first transaction's intent TTL (`1784643562`) falls between
the block timestamp (`1784643558`) and `parent + 12` (`1784643564`), so a fresh sync halts at
#1788979 with `Intent TTL has expired`.

A first transaction that fails `well_formed` at the corrected `tblock` is now retried at the
block's own `tblock`, matching the producer's cache-dependent behaviour at the time. The retry is
only reachable under host-function v1, so it never applies to blocks after the v2 runtime upgrade
(mainnet #2738210).

PR: https://github.com/midnightntwrk/midnight-node/pull/2238
Issue: https://github.com/midnightntwrk/midnight-node/issues/1924
