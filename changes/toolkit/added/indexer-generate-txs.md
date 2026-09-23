#toolkit #indexer
# Build `generate-txs` transactions from indexer state

`--indexer-url`, `--network` and `--indexer-concurrency` moved from `show-wallet` onto the shared
source arguments, and `generate-txs` now honours them: wallets are synced from the indexer
(resuming from the wallet cache, as `show-wallet` does) instead of fetching and replaying every
block. Block context and ledger parameters come from one tip snapshot per build.

`batches` and `send` are not yet supported with `--indexer-url` and fail with a clear error.
`register-dust-address` and contract calls with shielded inputs still hit the unimplemented
`backs_dust_generation` / `zswap_state` indexer methods. Only Schnorr seeds are supported.

`generate-intent circuit` now takes `--network` from the shared source arguments (same flag and
default), since the two definitions clashed.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
