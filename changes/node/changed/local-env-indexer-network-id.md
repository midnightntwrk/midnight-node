#node #fix
# local-env: set indexer NETWORK_ID=local for wallet-indexer and indexer-api

On the `local` network only `chain-indexer` set `APP__APPLICATION__NETWORK_ID`, so
`wallet-indexer` and `indexer-api` fell back to the default `undeployed` and
rejected the network's `mn_addr_local…` addresses ("expected HRP
`mn_addr_undeployed`, but was `mn_addr_local`"). This broke unshielded (NIGHT)
sync on the local network via the `unshieldedTransactions` subscription — it
presented as "network synced but no NIGHT balance". Set
`APP__APPLICATION__NETWORK_ID: "local"` on both services to match `chain-indexer`.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1956
