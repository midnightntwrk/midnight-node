# Register Dust Address Command in Toolkit

Update to the `generate-txs` command to include a builder for a transaction that registers a specified dust address. Funding for the transaction is provided by a different wallet, specified by `--funding-seed` as is the pattern for the other transactions.

For example:
```sh
cargo run -p midnight-node-toolkit -- generate-txs --src-file res/genesis/genesis_block_undeployed.mn  --dest-file register.mn --to-bytes register-dust-address --wallet-seed 0000000000000000000000000000000000000000000000000000000000000000 --funding-seed 0000000000000000000000000000000000000000000000000000000000000001
```


PR: https://github.com/midnightntwrk/midnight-node/pull/70
JIRA: https://shielded.atlassian.net/browse/PM-19777
