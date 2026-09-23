#toolkit #performance
# Resume indexer-backed `show-wallet` from a cached wallet state

`show-wallet --indexer-url` previously re-drained all three of the indexer's
subscriptions from the origin on every invocation. The dust drain in particular
is the *chain-wide* ledger-event log, so its cost grew with chain length rather
than with wallet activity — every run deserialized the whole log to reconstruct
one wallet.

Each stream's resume cursor is now persisted alongside the state it produced, in
the toolkit's existing wallet cache:

- shielded — the zswap `WalletState`, whose `first_free` *is* the cursor, so the
  two cannot drift;
- unshielded — the reconciled UTXO set plus the highest applied `transactionId`;
- dust — the `DustLocalState` as of the last applied ledger-event `id`, captured
  before `process_ttls`, which is a projection against the current tip and is
  re-applied on every load.

Entries are keyed by chain (block 1's hash, matching the replay path's
`chain_id`), seed and ledger generation, in a namespace disjoint from the replay
path's, so a cursor can never be served against the wrong chain or across the
v8→v9 hardfork. A shielded drain that ends with its merkle tree misaligned from
the last transaction's `zswapEndIndex` — which a `PartialSuccess` tail can cause
— declines to cache its half rather than persist a cursor that would double-apply
outputs on resume.

Caching is on by default and uses the existing `--ledger-state-db` /
`--fetch-cache` flags; `--fetch-cache inmemory` disables it, as it already did
for the replay path. Bumping the wallet cache format to v4 costs existing replay
caches one re-replay.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
