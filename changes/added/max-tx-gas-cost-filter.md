#node
# Transaction pool gas cost filtering

Added `--max-tx-gas-cost` CLI arg and `MAX_TX_GAS_COST` env var to reject midnight
transactions whose estimated gas cost exceeds a configurable limit. This allows node
operators to protect their nodes from expensive transactions at the pool gateway level.
The CLI arg takes precedence over the env var when both are set.

PR: shttps://github.com/midnightntwrk/midnight-node/pull/1251
