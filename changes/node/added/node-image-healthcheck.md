#node #docker

# Docker HEALTHCHECK in the node image

The node image now carries a `HEALTHCHECK` that polls the RPC `/health`
endpoint on `127.0.0.1:$RPC_PORT` (`9944` by default). Custom launch
configurations use the same `RPC_PORT` value for `--rpc-port`, keeping the
node and its liveness probe aligned while a syncing node stays healthy. A
direct `--rpc-port` override must set `RPC_PORT` to the same value.

PR: <link to PR>
