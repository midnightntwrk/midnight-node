#toolkit #indexer
# Honour or reject `--indexer-url` on every source command

Every command that takes the shared source arguments now either reads from the indexer when
`--indexer-url` is set or fails with a clear error. Previously they accepted the flag, ignored it
and replayed blocks.

- `contract-state` reads the contract state from the indexer, with no wallet sync.
- `dust-balance` syncs the seeds' wallets from the indexer. Its output matches the replay path.
- `generate-intent circuit` (`--wallet-seed`) and `generate-sample-intent` sync their wallets from
  the indexer. `generate-intent circuit` also takes the tip's ledger parameters from the indexer
  (unless `--custom-ledger-parameters` is given), so it no longer needs `--src-url`.
- `show-night-pools` and `fetch` reject `--indexer-url`. The NIGHT pools are chain-wide ledger
  state the indexer does not serve, and `fetch` exists to replay blocks.

`ecdsa:` seeds now work with `--indexer-url`. The indexer filters by address and doesn't care which
key-derivation scheme produced it, so each seed's wallet is built under its own scheme. As on the
replay path, ECDSA needs a ledger-9 chain.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
