# Single Transaction

Send a single transaction funded by one wallet to multiple destination wallets.

## Unshielded tokens

Send unshielded tokens to three destination addresses:

```console
$ midnight-node-toolkit generate-txs single-tx \
>   --fetch-cache inmemory \
>   --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
>   --unshielded-amount 10 \
>   --destination-address mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r \
>   --destination-address mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm \
>   --destination-address mn_addr_undeployed12vv6yst6exn50pkjjq54tkmtjpyggmr2p07jwpk6pxd088resqzqszfgak
...
```
