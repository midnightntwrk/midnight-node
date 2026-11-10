#toolkit #indexer
# Use Blockfrost as the `--indexer-url` backend

A Blockfrost URL (`https://midnight-<network>.blockfrost.io/api/v0`) is now accepted by
`--indexer-url`. It uses Blockfrost's endpoint layout (`/api/v0`, `/api/v0/ws`) and sends the
`project_id` header from `BLOCKFROST_PROJECT_ID`, on both the HTTP and WebSocket connections. A Blockfrost
URL without `BLOCKFROST_PROJECT_ID` fails before any request is made.

This is the only way to use the indexer path on mainnet, where Blockfrost now hosts the indexer.

PR: https://github.com/midnightntwrk/midnight-node/pull/2409
Issue: https://github.com/midnightntwrk/midnight-node/issues/1186
