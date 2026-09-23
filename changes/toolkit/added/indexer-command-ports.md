#toolkit #indexer
# Honour or reject `--indexer-url` on every source command

Every command that takes the shared source arguments now either reads from the indexer when
`--indexer-url` is set or fails with a clear error. Previously they accepted the flag, ignored it
and replayed blocks.

- `contract-state` reads the contract state from the indexer, with no wallet sync.
- `dust-balance` syncs the seeds' wallets from the indexer. Its output matches the replay path.
- `generate-intent circuit` (`--wallet-seed`) and `generate-sample-intent` sync their wallets from
  the indexer. `generate-intent circuit` still fetches ledger parameters from `--src-url` unless
  `--custom-ledger-parameters` is given.
- `show-night-pools` and `fetch` reject `--indexer-url`. The NIGHT pools are chain-wide ledger
  state the indexer does not serve, and `fetch` exists to replay blocks.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
