# Midnight Network Tools

A flexible set of tools for launching **well-known networks, custom networks, and dynamic local environments**, as well as **performing state changes** against those networks (image upgrades now, runtime upgrades and hard forks coming soon).

This project provides a unified way to spin up Midnight resources for development, testing, and experimentation.

---

## Features

- Launch dockerized **well-known Midnight networks** (e.g. `qanet`, `devnet`, `testnet-02`, etc.)
- Perform **state-changing operations** such as image upgrades (runtime upgrades and hard forks planned).
- Launch a fully **dynamic local environment** with sped-up Cardano resources for quick testing of Partner Chains/Cardano capabilities.

---

## Usage

All functionality is available via npm/yarn scripts defined in `package.json`.

### Launching Networks

You can run different Midnight networks locally with:

```bash
npm run run:qanet
npm run run:devnet
npm run run:testnet-02
npm run run:node-dev-01
```

### Upgrading Networks

You can also launch a network and immediately apply image upgrades:

```bash
npm run image-upgrade:qanet
npm run image-upgrade:devnet
npm run image-upgrade:testnet-02
npm run image-upgrade:node-dev-01
```

### Stopping Networks

To stop any running network:

```bash
npm run stop:qanet
npm run stop:devnet
npm run stop:testnet-02
npm run stop:node-dev-01
```

### Fork Testing

See [fork-testing.md](../docs/fork-testing.md)

### Local Environment

In addition to well-known networks, you can launch a dynamic local environment that connects multiple components together.

### Local env – step by step

When first run, all images are pulled from public repositories. This may take some time.

The stack is built and started. A Cardano node begins block production from a pre-configured genesis file (private testnet, no public connectivity).

Once Cardano is synced, Ogmios and DB-Sync connect and begin syncing.

pc-contracts-cli inserts D parameter values and registers Midnight Node keys with Cardano.

Once Postgres is populated, Midnight nodes begin block production after 2 main chain epochs.

Starting the environment

To start the environment via Earthly:

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

Stopping the environment

When stopping, volumes must also be wiped (persistent state is not supported yet).

```bash
earthly +stop-local-env-latest
```

# or

```bash
earthly +stop-local-env --NODE-IMAGE=ghcr.io/midnight-ntwrk/midnight-node:0.12.0
```
