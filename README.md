[![Nightly Build Status](https://github.com/midnightntwrk/midnight-node/actions/workflows/nightly-build-check.yml/badge.svg?branch=main&event=schedule)](https://github.com/midnightntwrk/midnight-node/actions/workflows/nightly-build-check.yml?query=branch%3Amain)

# Midnight Node

Implementation of the Midnight blockchain node, providing consensus, transaction processing, and privacy-preserving smart contract execution. The node enables participants to maintain both public blockchain state and private user state through zero-knowledge proofs.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                            Midnight Node                             │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │                            Runtime                             │  │
│  │                                                                │  │
│  │  ┌──────────────────────────────────────────────────────────┐  │  │
│  │  │                         Pallets                          │  │  │
│  │  │                                                          │  │  │
│  │  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐   │  │  │
│  │  │  │  Midnight   │  │   Native     │  │   Federated    │   │  │  │
│  │  │  │   System    │  │    Token     │  │   Authority    │   │  │  │
│  │  │  │             │  │ Observation  │  │                │   │  │  │
│  │  │  └─────────────┘  └──────────────┘  └────────────────┘   │  │  │
│  │  │                                                          │  │  │
│  │  │  ┌─────────────┐  ┌──────────────┐  ┌────────────────┐   │  │  │
│  │  │  │   Version   │  │   Midnight   │  │   Federated    │   │  │  │
│  │  │  │             │  │              │  │   Authority    │   │  │  │
│  │  │  │             │  │              │  │  Observation   │   │  │  │
│  │  │  └─────────────┘  └──────────────┘  └────────────────┘   │  │  │
│  │  └──────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │                        Node Services                           │  │
│  │                                                                │  │
│  │    ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │  │
│  │    │   RPC    │  │ Consensus│  │  Network │  │ Keystore │      │  │
│  │    │  Server  │  │   AURA   │  │   P2P    │  │          │      │  │
│  │    │          │  │  GRANDPA │  │          │  │          │      │  │
│  │    └──────────┘  └──────────┘  └──────────┘  └──────────┘      │  │
│  └────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ WebSocket RPC
                                    │ Port: 9944
                                    ▼
                         ┌──────────────────────┐
                         │   External Clients   │
                         │  (Wallets, Indexers, │
                         │     Applications)    │
                         └──────────────────────┘
```

## Components

### Runtime Pallets

Midnight Node includes six custom runtime pallets that implement core blockchain functionality:

**[pallet-midnight](pallets/midnight)** - Core pallet managing ledger state and transaction execution
- Processes privacy-preserving smart contract transactions
- Maintains ledger state root and provides state access interface
- Integrates with midnight-ledger for zero-knowledge proof verification

**[pallet-midnight-system](pallets/midnight-system)** - System transaction management
- Handles administrative operations requiring root privileges
- Applies system-level transactions to ledger state

**[pallet-native-token-observation](pallets/native-token-observation)** - Cardano bridge integration
- Tracks cNIGHT token registration from Cardano mainchain
- Manages DUST generation and UTXO tracking
- Processes Cardano Midnight System Transactions (CMST)

**[pallet-federated-authority](pallets/federated-authority)** - Multi-collective governance
- Requires consensus from multiple authority bodies for critical operations
- Motion-based proposal system with time limits
- Executes approved motions with root privileges

**[pallet-federated-authority-observation](pallets/federated-authority-observation)** - Governance synchronization
- Observes authority changes from mainchain
- Updates Council and Technical Committee memberships
- Propagates governance changes across the network

**[pallet-version](pallets/version)** - Runtime version tracking
- Records runtime spec version in block digests
- Enables version monitoring and upgrade tracking

### Node Services

**RPC Server** - WebSocket endpoint (default port 9944) for client connections

**Consensus** - Hybrid consensus mechanism:
- AURA for block production (6-second blocks)
- GRANDPA for Byzantine-fault-tolerant finality
- BEEFY for bridge security
- MMR for efficient light client proofs

**Network** - P2P networking via libp2p (default port 30333)

**Keystore** - Local cryptographic key management for validators

## Features

**Privacy-Preserving Smart Contracts** - Execute contracts with zero-knowledge proofs while maintaining public blockchain state

**Partner Chain Architecture** - Integrated with Cardano mainchain as a partner chain with cross-chain token bridging (cNIGHT to DUST)

**Multi-Layer Governance** - Federated authority system requiring consensus from multiple governance bodies with automatic mainchain synchronization

**High Performance** - 6-second block time with efficient finality mechanism and optimized transaction processing

**Developer Tools** - Comprehensive CLI with chain specification generation, runtime benchmarking, and upgrade testing capabilities

## Quick Start

If you just want to run midnight-node, the easiest option
is to `git clone https://github.com/midnightntwrk/midnight-node-docker` and run the docker compose script.

## **Note on Open Sourcing Progress**

While this repository is open source, it depends on some repositories
that we are still in the process of being release. As such:

- It's not possible to compile midnight-node independently.
- If you raise a PR, the CI will be able to compile it.
- We're actively working to open-source dependencies in the coming months.

## Documentation

[Proposals](docs/proposals)
[Decisions](docs/decisions)

- [Development Workflow](docs/development-workflow.md) - Best practices for cargo vs earthly, debugging, and common tasks
- [Rust Installation](docs/rust-setup.md) - Complete setup including GitHub access, Nix, and Direnv
- [Configuration](docs/config.md) - Node configuration options
- [Chain Specifications](docs/chain_specs.md) - Working with different networks
- [Block Weights](docs/weights.md) - Runtime weights documentation
- [Actionlint Guide](docs/actionlint-guide.md) - GitHub Actions validation

## Prerequisites

- rustup installed
- For any docker steps: [Docker](https://docs.docker.com/get-docker/)
  and [Docker Compose](https://docs.docker.com/compose/install/) (or podman).
- [Earthly](https://earthly.dev/get-earthly) - containerized build system
- [Direnv](https://direnv.net/docs/installation.html) - manages environment variables
- Netrc file with git credentials. See this [reference setup](https://gist.github.com/technoweenie/1072829)

## Contributing

[Guide lines on contributing](./CONTRIBUTING.md).

## Development Workflow

See [docs/development-workflow.md](docs/development-workflow.md) for complete workflow guidance including:
- Environment setup (Nix, Direnv, or manual)
- Cargo vs Earthly best practices (when to use each)
- Common development tasks and commands
- Ledger upgrade procedures
- Debugging tips and techniques
- Chain specification workflow
- AWS secrets workflow

For quick earthly target reference, run `earthly doc` to list all available targets.

## How-To Guides

### Rebuilding preprod/prod genesis

For `preprod` and `prod` chains, node keys and wallet seeds used in genesis are stored as AWS secrets.

**Working without AWS access:**

If you don't have AWS access, you can still rebuild chainspecs without rebuilding the genesis, since the public keys for the initial authority nodes are stored in `/res/$NETWORK_NAME/initial-authorities.json`:

```shell
$ earthly +rebuild-chainspecs
```

For local development without secrets, use the `undeployed` network.

**Working with AWS access:**

If you have AWS access, you can perform full genesis rebuilds:

1. Copy secrets from AWS into the `/secrets` directory:
   ```shell
   # Example for testnet
   secrets/testnet-seeds-aws.json
   secrets/testnet-keys-aws.json
   ```

2. Regenerate the mock file:
   ```shell
   $ earthly +generate-keys
   # Output: /res/testnet/initial-authorities.json and /res/mock-bridge-data/testnet-mock.json
   ```

3. Rebuild genesis for a preprod environment:
   ```shell
   # secrets copied from /secrets/testnet-02-genesis-seeds.json
   $ earthly +rebuild-genesis-testnet-02
   ```

4. (Optional) Regenerate the genesis seeds:
   ```shell
   $ earthly +generate-testnet-02-genesis-seeds
   ```

**Need genesis rebuilt but don't have AWS access?**

Contact the node team in Slack. Provide:
- Your PR number
- Which network needs genesis rebuilt (qanet/preview/testnet)
- Confirmation that you've committed all necessary changes

A team member with AWS access will download the secrets and run the rebuild command for you.

### How to use transaction generator in the midnight toolkit

See this [document](util/toolkit/README.md)

### Build Docker images

These are built in CI. See the workflow files for the latest `earthly` commands:

- [node](.github/workflows/main.yml)
- [toolkit](.github/workflows/main.yml)

### Start local network

**Available Networks:**
- `local` - Development network (default)
- `qanet` - QA testing network
- `preview` - Preview/staging network
- `perfnet` - Performance testing network

Chain specifications are located in `/res/` directory.

**Configuration Parameters:**

| Parameter | Environment Variable | CLI Flag | Description |
|-----------|---------------------|----------|-------------|
| Config preset | `CFG_PRESET=dev` | - | Development mode configuration |
| Validator seed | `SEED=//Alice` | - | Validator identity (//Alice, //Bob, etc.) |
| Chain spec | - | `--chain local` | Network to connect to |
| Base path | - | `--base-path /tmp/node-1` | Data directory |
| Validator mode | - | `--validator` | Run as validator |
| P2P port | - | `--port 30333` | Networking port (default: 30333) |
| RPC port | - | `--rpc-port 9944` | WebSocket RPC port (default: 9944) |
| Node key | - | `--node-key "0x..."` | Network identity key |
| Bootstrap nodes | - | `--bootnodes "/ip4/..."` | Initial peers |

**Start single-node local network** for development:

```shell
CFG_PRESET=dev SEED=//Alice ./target/release/midnight-node --base-path /tmp/node-1 --chain local --validator
```

**Start multi-node local network** with 5/7 authority nodes using the `local` chain specification:

```shell
CFG_PRESET=dev SEED=//Alice ./target/release/midnight-node --base-path /tmp/node-1 --node-key="0000000000000000000000000000000000000000000000000000000000000001" --validator --port 30333
CFG_PRESET=dev SEED=//Bob ./target/release/midnight-node --base-path /tmp/node-2 --node-key="0000000000000000000000000000000000000000000000000000000000000002" --bootnodes "/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp"  --validator --port 30334
CFG_PRESET=dev SEED=//Charlie ./target/release/midnight-node --base-path /tmp/node-3 --node-key="0000000000000000000000000000000000000000000000000000000000000003" --bootnodes "/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" --validator --port 30335
CFG_PRESET=dev SEED=//Dave ./target/release/midnight-node --base-path /tmp/node-4 --node-key="0000000000000000000000000000000000000000000000000000000000000004" --bootnodes "/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" --validator --port 30336
CFG_PRESET=dev SEED=//Eve ./target/release/midnight-node --base-path /tmp/node-5 --node-key="0000000000000000000000000000000000000000000000000000000000000005" --bootnodes "/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" --validator --port 30337
CFG_PRESET=dev SEED=//Ferdie ./target/release/midnight-node --base-path /tmp/node-6 --node-key="0000000000000000000000000000000000000000000000000000000000000006" --bootnodes "/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" --validator --port 30338
```

### How to build runtime in Docker

```shell
earthly +build
cp ./artifacts-amd64/midnight-node-runtime/target/wasm32-unknown-unknown/release/midnight_node_runtime.wasm  .
```

### How to generate node public keys

- For generating single keys:
    - Build node and then run:

```shell
./target/release/midnight-node key generate
```

See the `--help` flag for more information on other arguments, including key schemes.

- For generating multiple keys for bootstrapping:
    - Run the following script to generate $n$ number of key triples and seed phrases. The triples are formatted as
      Rust `enum`s for easy pasting into chain spec files, in the order: `(aura, grandpa, cross_chain)`

```shell
python ./scripts/generate-keys.py --help
```

### Fork Testing

See [fork-testing.md](../docs/fork-testing.md)
