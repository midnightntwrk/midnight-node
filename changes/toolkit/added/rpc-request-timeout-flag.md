# Configurable node RPC request timeout

New global `--rpc-request-timeout <secs>` flag (env: `MN_RPC_REQUEST_TIMEOUT`)
for all toolkit commands that talk to a node. Previously the request timeout
was fixed at jsonrpsee's 60 s default, which heavy requests such as
`Metadata_metadata_at_version` can exceed on slow hardware or a loaded node,
killing commands like `generate-txs register-dust-address` mid-run.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1853
