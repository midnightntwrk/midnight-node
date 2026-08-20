# Embedded indexer

`midnight-node` can run the wallet-facing indexer and GraphQL API in the node process. The feature
is disabled unless the node is started with `--indexer`:

```shell
CONFIG_FILE=/etc/midnight/indexer.yaml midnight-node --indexer
```

Use this on dedicated API nodes rather than validators. The embedded indexer retains the current
indexer behavior: it connects to the node's WebSocket RPC, replays ledger transactions into SQLite,
and serves the GraphQL API on the address configured under `infra.api`.

The configuration schema and `APP__` environment overlays are identical to the standalone
indexer. `CONFIG_FILE` selects its YAML file and defaults to `config.yaml`. The node container
image sets it to a bundled copy of
[`indexer-standalone/config.yaml`](../indexer/indexer-standalone/config.yaml). For other packaging,
start from that file. Set writable and distinct paths for `infra.storage.cnn_url` and
`infra.ledger_db.cnn_url`, and provide secrets with the existing `APP__*_FILE` mechanism.

The indexer runs as a supervised, non-essential node task. Invalid configuration or an indexer
component failure leaves the node available and restarts the indexer after a short delay. The
indexer's `/ready` endpoint remains unhealthy until indexing has recovered and caught up, so route
wallet traffic using that readiness signal rather than node RPC readiness. Process-wide logging and
tracing remain owned by `midnight-node`; indexer metrics keep their configured listener.
