# Native indexer

`midnight-node` can run the wallet-facing indexer and GraphQL API. The feature is disabled unless
the node is started with `--indexer`:

```shell
CONFIG_FILE=/etc/midnight/indexer.yaml midnight-node --indexer
```

Use this on dedicated API nodes rather than validators. The native indexer retains the current
indexer behavior: it connects to the node's WebSocket RPC, replays ledger transactions into SQLite,
and serves the GraphQL API on the address configured under `infra.api`.

The indexer source is maintained directly under `indexer/` in this repository. It is not a Git
submodule and does not require a separately released or deployed indexer component. The node image
contains a separately linked worker so historical ledger dependencies retain their proven lockfile;
`midnight-node` starts, monitors, and stops that worker as part of `--indexer` mode.

The configuration schema and `APP__` environment overlays are identical to the former standalone
indexer. `CONFIG_FILE` selects its YAML file and defaults to `config.yaml`. The node container image
sets it to a bundled copy of
[`indexer-standalone/config.yaml`](../indexer/indexer-standalone/config.yaml). For other packaging,
start from that file. Set writable and distinct paths for `infra.storage.cnn_url` and
`infra.ledger_db.cnn_url`, and provide secrets with the existing `APP__*_FILE` mechanism.

The indexer worker is supervised but non-essential to node consensus. Invalid configuration or an
indexer component failure leaves the node available and restarts the worker after a short delay. The
indexer's `/ready` endpoint remains unhealthy until indexing has recovered and caught up, so route
wallet traffic using that readiness signal rather than node RPC readiness.
