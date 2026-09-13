# Add Ledger Sync process for Warp Sync

Adds support for syncing ledger state, while warp syncing. Should enable Substrate warp sync.
Non-validator nodes serve ledger snapshots to warp-syncing peers; validators don't by default,
but can opt in via the new `--serve-warp-ledger-sync` flag (off by default), and any node can opt
out with `--no-serve-warp-ledger-sync`. Nodes can warp-sync as clients regardless of either flag.

Serving is bounded against abuse: snapshots are memoized in a small LRU, peers that replay an
identical byte range are penalised, and each peer has a budget for how many full-arena
serializations it may induce, charged before the work rather than after. On the client side each
range request has its own timeout and the whole per-peer transfer has a throughput floor, so one
slow peer cannot hold arena recovery open indefinitely.

PR: https://github.com/midnightntwrk/midnight-node/pull/1650
Issue: https://github.com/midnightntwrk/midnight-node/issues/1648
