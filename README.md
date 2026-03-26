[![Nightly Build Status](https://github.com/midnightntwrk/midnight-node/actions/workflows/nightly-build-check.yml/badge.svg?branch=main&event=schedule)](https://github.com/midnightntwrk/midnight-node/actions/workflows/nightly-build-check.yml?query=branch%3Amain)

# Midnight Node

Implementation of the Midnight blockchain node, providing consensus, transaction processing, and privacy-preserving smart contract execution. The node enables participants to maintain both public blockchain state and private user state through zero-knowledge proofs.

## Architecture

```
┌────────────────────────────────────────────────────────────────────────────┐
│                        Midnight Node Wizard                                │
└────────────────────────────────────────────────────────────────────────────┘
         │
         │ Register Partner Chain
         ▼
┌─────────────┐      ┌─────────────────┐      ┌──────────────┐
│   Cardano   │ ───▶ │ Cardano Indexer │ ───▶ │  PostgreSQL  │
│  Mainchain  │      │ (db-sync)       │      │  (cexplorer) │
└─────────────┘      └─────────────────┘      └──────────────┘
                                                      │ Observes mainchain state
                                                      │ Queries Cardano data
                                                      │ (cNIGHT, governance)
                                                      ▼
     ┌────────────────────────────────────────────────────────────────────┐
     │                         Midnight Node                              │
     ├────────────────────────────────────────────────────────────────────┤
     │                                                                    │
     │  ┌──────────────────────────────────────────────────────────────┐  │
     │  │                          Runtime                             │  │
     │  │                                                              │  │
     │  │  ┌────────────────────────────────────────────────────────┐  │  │
     │  │  │                       Pallets                          │  │  │
     │  │  │                                                        │  │  │
     │  │  │  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐   │  │  │
     │  │  │  │  Midnight   │  │   Native     │  │  Federated   │   │  │  │
     │  │  │  │   System    │  │    Token     │  │  Authority   │   │  │  │
     │  │  │  │             │  │ Observation  │  │              │   │  │  │
     │  │  │  └─────────────┘  └──────────────┘  └──────────────┘   │  │  │
     │  │  │                                                        │  │  │
     │  │  │  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐   │  │  │
     │  │  │  │   Version   │  │   Midnight   │  │  Federated   │   │  │  │
     │  │  │  │             │  │              │  │  Authority   │   │  │  │
     │  │  │  │             │  │              │  │ Observation  │   │  │  │
     │  │  │  └─────────────┘  └──────────────┘  └──────────────┘   │  │  │
     │  │  └────────────────────────────────────────────────────────┘  │  │
     │  └──────────────────────────────────────────────────────────────┘  │
     │                                                                    │
     │  ┌──────────────────────────────────────────────────────────────┐  │
     │  │                      Node Services                           │  │
     │  │                                                              │  │
     │  │    ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │  │
     │  │    │   RPC    │  │Consensus │  │ Keystore │  │ Network  │    │  │
     │  │    │  Server  │  │   AURA   │  │          │  │   P2P    │◀───│──│────▶ Other Midnight Nodes
     │  │    │          │  │ GRANDPA  │  │          │  │Port 30333│    │  │
     │  │    └──────────┘  └──────────┘  └──────────┘  └──────────┘    │  │
     │  └──────────────────────────────────────────────────────────────┘  │
     └────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ WebSocket RPC
                                    │ Port: 9944
                                    ▼
            ┌─────────────────────────────────────────────────────────┐
            │   External Clients: Apps, Indexers [1]                  │
            └─────────────────────────────────────────────────────────┘
```
[1] [Midnight Indexer](https://github.com/midnightntwrk/midnight-indexer)

> **Security Note:** Database connections to PostgreSQL require SSL/TLS by default. Set `ALLOW_NON_SSL=true` only for local development environments without SSL certificates.
> 
> Please also see https://docs.polkadot.com/infrastructure/running-a-validator/onboarding-and-offboarding/set-up-validator/ for further security recommendations on running validators.

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

### Cardano Smart Contracts

We make use of several smart contracts on Cardano to support Midnight functionality. These can be found in [midnight-reserve-contracts](https://github.com/midnightntwrk/midnight-reserve-contracts). These are built in verbose mode using the command:

```shell
$ ./build_contracts.sh <network> verbose
```

- `cnight-mapping-validator.ak`@[f11d27828666e887fb495a85242edf9b8a78192f`](https://github.com/midnightntwrk/midnight-reserve-contracts/commit/f11d27828666e887fb495a85242edf9b8a78192f) provides the mapping_validator_address  "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng"
- `test_cnight_no_audit.ak`@[f11d27828666e887fb495a85242edf9b8a78192f`](https://github.com/midnightntwrk/midnight-reserve-contracts/commit/f11d27828666e887fb495a85242edf9b8a78192f) provides the tcnight policy id  "d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1"

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
- [OpenRPC API Specification](docs/openrpc.md) - Machine-readable API schema via `rpc.discover`
- [Configuration Guide](docs/configuration-guide.md) - Comprehensive configuration guide for SREs
- [Rust Installation](docs/rust-setup.md) - Setup instructions and toolchain information
- [Chain Specifications](docs/chain_specs.md) - Working with different networks
- [Block Weights](docs/weights.md) - Runtime weights documentation
- [Actionlint Guide](docs/actionlint-guide.md) - GitHub Actions validation
- [Governance](docs/governance/overview.md) - Federated Authority Governance System documentation
  - [Runtime Upgrade Guide](docs/governance/example/runtime-upgrade.md) - Step-by-step guide for runtime upgrades via governance
- [Security](docs/security/image-signing.md) - Container image signing and verification
  - [Verification Guide](docs/security/verification-guide.md) - How to verify image signatures and SBOMs
  - [Signing Runbook](docs/security/signing-runbook.md) - Operational procedures for signing
- [Operations](docs/operations/release-checklist.md) - Release checklist with security verification steps

## Prerequisites

- rustup installed
- For any docker steps: [Docker](https://docs.docker.com/get-docker/)
  and [Docker Compose](https://docs.docker.com/compose/install/) (or podman).
- [Earthly](https://earthly.dev/get-earthly) - containerized build system
- [Direnv](https://direnv.net/docs/installation.html) - manages environment variables

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

| Parameter | Environment Variable | CLI Flag (Alternative) | Description |
|-----------|---------------------|------------------------|-------------|
| Config preset | `CFG_PRESET=dev` | - | Development mode configuration |
| AURA seed | `AURA_SEED_FILE=/path/to/seed` | - | Path to AURA consensus seed file |
| GRANDPA seed | `GRANDPA_SEED_FILE=/path/to/seed` | - | Path to GRANDPA finality seed file |
| Cross-chain seed | `CROSS_CHAIN_SEED_FILE=/path/to/seed` | - | Path to cross-chain seed file |
| Chain spec | `CHAIN=local` | `--chain local` | Network to connect to |
| Base path | `BASE_PATH=/tmp/node-1` | `--base-path /tmp/node-1` | Data directory |
| Validator mode | `VALIDATOR=true` | `--validator` | Run as validator (true/1/TRUE) |
| P2P port | - | `--port 30333` | Networking port (default: 30333) |
| RPC port | - | `--rpc-port 9944` | WebSocket RPC port (default: 9944) |
| Node key | `NODE_KEY_FILE=/path/to/key` | `--node-key "0x..."` | Network identity key file |
| Bootstrap nodes | `BOOTNODES="/ip4/... /ip4/..."` | `--bootnodes "/ip4/..."` | Space-separated initial peers |
| Allow non-SSL DB | `ALLOW_NON_SSL=false` | - | Allow non-SSL PostgreSQL connections |
| Remote write | `PROMETHEUS_PUSH_ENDPOINT=https://thanos:9091/api/v1/receive` | - | Push metrics via Prometheus Remote Write (Thanos, Cortex, Mimir) |
| Push interval | `PROMETHEUS_PUSH_INTERVAL_SECS=15` | - | Seconds between metric pushes (default: 15) |
| Push job name | `PROMETHEUS_PUSH_JOB_NAME=midnight-node` | - | Job label for pushed metrics (default: midnight-node) |

**Start single-node local network** for development:

```shell
echo "//Alice" > /tmp/alice-seed && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/alice-seed GRANDPA_SEED_FILE=/tmp/alice-seed CROSS_CHAIN_SEED_FILE=/tmp/alice-seed \
  BASE_PATH=/tmp/node-1 CHAIN=local VALIDATOR=true ./target/release/midnight-node
```

**Start multi-node local network** with 6/7 authority nodes using the `local` chain specification:

```shell
echo "//Alice" > /tmp/alice-seed && echo "0000000000000000000000000000000000000000000000000000000000000001" > /tmp/alice-key && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/alice-seed GRANDPA_SEED_FILE=/tmp/alice-seed CROSS_CHAIN_SEED_FILE=/tmp/alice-seed \
  NODE_KEY_FILE=/tmp/alice-key BASE_PATH=/tmp/node-1 CHAIN=local VALIDATOR=true ./target/release/midnight-node --port 30333

echo "//Bob" > /tmp/bob-seed && echo "0000000000000000000000000000000000000000000000000000000000000002" > /tmp/bob-key && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/bob-seed GRANDPA_SEED_FILE=/tmp/bob-seed CROSS_CHAIN_SEED_FILE=/tmp/bob-seed \
  NODE_KEY_FILE=/tmp/bob-key BASE_PATH=/tmp/node-2 CHAIN=local VALIDATOR=true \
  BOOTNODES="/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  ./target/release/midnight-node --port 30334

echo "//Charlie" > /tmp/charlie-seed && echo "0000000000000000000000000000000000000000000000000000000000000003" > /tmp/charlie-key && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/charlie-seed GRANDPA_SEED_FILE=/tmp/charlie-seed CROSS_CHAIN_SEED_FILE=/tmp/charlie-seed \
  NODE_KEY_FILE=/tmp/charlie-key BASE_PATH=/tmp/node-3 CHAIN=local VALIDATOR=true \
  BOOTNODES="/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  ./target/release/midnight-node --port 30335

echo "//Dave" > /tmp/dave-seed && echo "0000000000000000000000000000000000000000000000000000000000000004" > /tmp/dave-key && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/dave-seed GRANDPA_SEED_FILE=/tmp/dave-seed CROSS_CHAIN_SEED_FILE=/tmp/dave-seed \
  NODE_KEY_FILE=/tmp/dave-key BASE_PATH=/tmp/node-4 CHAIN=local VALIDATOR=true \
  BOOTNODES="/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  ./target/release/midnight-node --port 30336

echo "//Eve" > /tmp/eve-seed && echo "0000000000000000000000000000000000000000000000000000000000000005" > /tmp/eve-key && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/eve-seed GRANDPA_SEED_FILE=/tmp/eve-seed CROSS_CHAIN_SEED_FILE=/tmp/eve-seed \
  NODE_KEY_FILE=/tmp/eve-key BASE_PATH=/tmp/node-5 CHAIN=local VALIDATOR=true \
  BOOTNODES="/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  ./target/release/midnight-node --port 30337

echo "//Ferdie" > /tmp/ferdie-seed && echo "0000000000000000000000000000000000000000000000000000000000000006" > /tmp/ferdie-key && \
CFG_PRESET=dev AURA_SEED_FILE=/tmp/ferdie-seed GRANDPA_SEED_FILE=/tmp/ferdie-seed CROSS_CHAIN_SEED_FILE=/tmp/ferdie-seed \
  NODE_KEY_FILE=/tmp/ferdie-key BASE_PATH=/tmp/node-6 CHAIN=local VALIDATOR=true \
  BOOTNODES="/ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp" \
  ./target/release/midnight-node --port 30338
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

See [fork-testing.md](docs/fork-testing.md)
