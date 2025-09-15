# Midnight Local Environment

This stack is designed to run a 5 x node local environment for a Midnight chain. The environment is designed to be a self-contained, local development environment for testing and development of the Midnight Node and the Cardano main chain.

The local environment includes:

- 5 x Midnight Nodes
- 1 x Cardano Node running private testnet with pre-configured genesis files (2 minutes epochs)
- 1 x PostgreSQL database
- 1 x Db-sync
- 1 x Ogmios
- 1 x Ubuntu / NodeJS image for running pc-contracts-cli

## Local env - step by step

- When first run, all images will be pulled from public repositories. This stage may take some time. The stack will then be built and run.
- When the stack is running, the Cardano node begins block production. This is a private testnet and will not connect to the public Cardano network, but rather from a pre-configured genesis file.
- Once the Cardano chain is synced, Ogmios and DB-Sync will in turn connect to the Cardano node node.socket and begin syncing the chain.
- The pc-contracts-cli will insert D parameter values and register Midnight Node keys with the Cardano chain.
- Once Postgres is populated with the required data, the Midnight nodes will begin block production after 2 main chain epochs.

## Starting the environment

To start the environment, use the Earthly target in the Earthfile at the root of this repo:

```
earthly +start-local-env-latest
```

To specify a released node image, use the `+start-local-env` target with the `NODE_IMAGE` arg:

```
earthly +start-local-env --NODE-IMAGE=ghcr.io/midnight-ntwrk/midnight-node:0.12.0
```

We recommend using a visual Docker UI tool such as [lazydocker](https://github.com/jesseduffield/lazydocker) or [Docker Desktop](https://www.docker.com/products/docker-desktop/) for following the live logs and performance of all containers in the environment. Each component has been scripted to provide verbose logging of all configuration actions it is performing to demonstrate the end-to-end setup of a Midnight testnet

## Stopping the environment

When stopping the stack, it is mandatory to also wipe all volumes. The environment does not yet support persistent state. To tear down the environment and remove all volumes, use following Earthly targets:

```
earthly +stop-local-env-latest
# or
earthly +stop-local-env --NODE-IMAGE=ghcr.io/midnight-ntwrk/midnight-node:0.12.0
```

Make sure you use the `stop` target with the same args as your `start` target.
