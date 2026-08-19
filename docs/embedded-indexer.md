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

The indexer is an essential node task. Invalid configuration or an indexer component failure stops
the dedicated node so its process supervisor can restart it rather than leaving a false-healthy RPC
node without the wallet API. Process-wide logging and tracing remain owned by `midnight-node`;
indexer metrics keep their configured listener.
