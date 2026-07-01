# Midnight Network Tools

Tools for bringing up local forks of well-known Midnight networks and rehearsing
upgrade flows against them.

## Features

- Restore a well-known network from an `http(s)` snapshot and replace the live
  authority set with locally generated mock validators.
- Reuse an existing restored fork across `run`, `image-upgrade`,
  `governance-runtime-upgrade`, and `full-upgrade` commands without
  re-downloading the snapshot each time.
- Rehearse image-only upgrades, governance runtime upgrades, or a two-phase
  rollout that does both in sequence.
- Launch the standalone `local-env` stack for fast local Partner Chains testing.

## Usage

All commands are exposed through the npm scripts in
[package.json](./package.json).

### Well-known network forks

Supported fork targets are `devnet`, `qanet`, `testnet-02`, `preview`,
`preprod`, and `mainnet`.

The first bring-up for a well-known network needs `--from-snapshot`. The CLI
downloads the archive, restores it into each compose data directory, runs
`mock-authorities convert`, and writes a compose override that switches the fork
into mock-validator mode.

```bash
npm run run:preview -- --from-snapshot https://example.com/snapshots/preview-latest.tar.zst
npm run run:preprod -- --from-snapshot https://example.com/snapshots/preprod-latest.tar.gz
npm run run:mainnet -- --from-snapshot https://example.com/snapshots/mainnet-latest.tar.gz
```

After that initial restore, the same network can be restarted without
`--from-snapshot` as long as the restored `data/` directories and generated
mock-authorities output are still present:

```bash
npm run run:preview
```

Before forking from a snapshot, confirm the chainspec embedded in the node
image was built with the same `networkId` as the genesis used to produce the
snapshot. Recent runtimes validate this at boot and the node will refuse to
start on a mismatch.

### Upgrade rehearsals

`image-upgrade` rolls service containers from `NODE_IMAGE` to `NEW_NODE_IMAGE`.

```bash
NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:old \
NEW_NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:new \
npm run image-upgrade:preview -- --from-snapshot https://example.com/snapshots/preview-latest.tar.zst
```

`governance-runtime-upgrade` submits the federated-authority flow against a
running fork. The wasm path must resolve under the repo-level `artifacts/`
directory.

```bash
npm run governance-runtime-upgrade:preview -- \
  --wasm upgrade/midnight_node_runtime.compact.wasm \
  --council-uris //Dave //Eve //Ferdie \
  --technical-uris //Alice //Bob //Charlie \
  --executor-uri //Alice
```

`full-upgrade` runs the production-shaped rehearsal: first the image rollout,
then the governance runtime upgrade against the running fork.

```bash
NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:old \
NEW_NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:new \
npm run full-upgrade:preview -- \
  --from-snapshot https://example.com/snapshots/preview-latest.tar.zst \
  --wasm upgrade/midnight_node_runtime.compact.wasm \
  --council-uris //Dave //Eve //Ferdie \
  --technical-uris //Alice //Bob //Charlie \
  --executor-uri //Alice
```

Use `--allow-same-version` only for local rehearsals where the candidate wasm
does not bump `spec_version`. It deliberately bypasses the runtime-side version
check and should not be used for production-shaped validation.

### Stopping networks

```bash
npm run stop:preview
npm run stop:preprod
npm run stop:mainnet
```

### Fork testing

See [fork-testing.md](../docs/fork-testing.md) for snapshot prerequisites and
archive format details.

### Local environment

In addition to the fork-based workflows above, you can launch a dynamic local
environment that connects multiple components together.

### Local env - step by step

> **Note:** The governance contracts are tracked as a git submodule at
> `midnight-reserve-contracts/`. If you cloned without `--recurse-submodules`,
> run:
>
> ```bash
> git submodule update --init midnight-reserve-contracts
> ```
>
> The submodule pin is the version used in CI; do not edit it on the local-env
> path.

> **Note:** Local development environments use a self-signed TLS certificate for
> PostgreSQL connections. Production deployments should set `ssl_root_cert` for
> full certificate validation (`PgSslMode::VerifyFull`).

When first run, all images are pulled from public repositories. This may take
some time.

The stack is built and started. A Cardano node begins block production from a
pre-configured genesis file (private testnet, no public connectivity).

Once Cardano is synced, Ogmios and DB-Sync connect and begin syncing.

`pc-contracts-cli` inserts D parameter values and registers Midnight Node keys
with Cardano.

Once Postgres is populated, Midnight nodes begin block production after 2 main
chain epochs.

Starting the environment via Earthly:

```bash
earthly +start-local-env-latest
```

Or specify a released node image:

```bash
earthly +start-local-env --NODE-IMAGE=ghcr.io/midnight-ntwrk/midnight-node:0.12.0
```

You can also use npm scripts:

```bash
npm run run:local-env
npm run run:local-env-with-indexer
```

Stopping the environment:

When stopping, volumes must also be wiped (persistent state is not supported
yet).

```bash
earthly +stop-local-env-latest
```

Or:

```bash
earthly +stop-local-env --NODE-IMAGE=ghcr.io/midnight-ntwrk/midnight-node:0.12.0
```
