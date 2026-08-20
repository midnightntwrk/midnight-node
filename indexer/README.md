# Midnight Indexer

The Midnight Indexer (midnight-indexer) is a set of components designed to optimize the flow of blockchain data from a Midnight Node to end-user applications. It retrieves the history of blocks, processes them, stores indexed data efficiently, and provides a GraphQL API for queries and subscriptions. This Rust-based implementation is the next-generation iteration of the previous Scala-based indexer, offering improved performance, modularity, and ease of deployment.
```
                                ┌─────────────────┐
                                │                 │
                                │                 │
                                │      Node       │
                                │                 │
                                │                 │
                                └─────────────────┘
                                         │
┌────────────────────────────────────────┼────────────────────────────────────────┐
│                                        │                                        │
│                                        │ fetch                                  │
│                                        │ blocks                                 │
│                                        ▼                                        │
│                               ┌─────────────────┐                               │
│                               │                 │                               │
│                               │                 │                               │
│                               │      Chain      │                               │
│             ┌─────────────────│     Indexer     │                               │
│             │                 │                 │                               │
│             │                 │                 │                               │
│             │                 └─────────────────┘                               │
│             │                          │                                        │
│             │                          │ save blocks                            │
│             │                          │ and transactions                       │
│             │    save relevant         ▼                                        │
│             │     transactions   .───────────.                                  │
│      notify │        ┌─────────▶(     DB      )───────────────────┐             │
│ transaction │        │           `───────────'                    │             │
│     indexed │        │                                            │ read data   │
│             ▼        │                                            ▼             │
│    ┌─────────────────┐                                   ┌─────────────────┐    │
│    │                 │                                   │                 │    │
│    │                 │                                   │                 │    │
│    │     Wallet      │                                   │     Indexer     │    │
│    │     Indexer     │◀──────────────────────────────────│       API       │    │
│    │                 │  notify                           │                 │    │
│    │                 │  wallet                           │                 │    │
│    └─────────────────┘  connected                        └─────────────────┘    │
│                                                                   ▲             │
│                                                           connect │             │
│                                                                   │             │
└───────────────────────────────────────────────────────────────────┼─────────────┘
                                                                    │
                                 ┌─────────────────┐                │
                                 │                 │                │
                                 │                 │                │
                                 │     Wallet      │────────────────┘
                                 │                 │
                                 │                 │
                                 └─────────────────┘
```

## Components

- [Chain Indexer](chain-indexer/README.md): Connects to the Midnight Node, fetches blocks and transactions, and stores indexed data.
- [Wallet Indexer](wallet-indexer/README.md): Associates connected wallets with relevant transactions, enabling personalized queries and subscriptions.
- [Indexer API](indexer-api/README.md): Exposes a GraphQL API for queries, mutations, and subscriptions.
- [SPO Indexer](spo-indexer/README.md): Indexes stake-pool operator data - committee membership, epoch and D-parameter changes, and Cardano stake distribution (via Blockfrost).

## Features

- Fetch and query blocks, transactions and contract actions at specific block hashes, heights, transaction identifiers or contract addresses.
- Real-time subscriptions to new blocks, contract actions and wallet-related events through WebSocket connections.
- Secure wallet sessions enabling clients to track only their relevant transactions.
- Configurable for both cloud (microservices) and standalone (single binary) deployments.
- Supports both PostgreSQL (cloud) and SQLite (standalone) storage backends.
- Extensively tested with integration tests and end-to-end scenarios.

## Running

To run the Midnight Indexer Docker images are provided under the [`midnightntwrk`](https://hub.docker.com/r/midnightntwrk) organization. It is supposed that users are familiar with running Docker images, e.g. via Docker Compose or Kubernetes.

### Standalone Mode

The standalone Indexer combines the Chain Indexer, Indexer API, Wallet Indexer and SPO Indexer components in a single executable alongside an in-process SQLite database. Therefore the only Docker image to be run is [`indexer-standalone`](https://hub.docker.com/r/midnightntwrk/indexer-standalone).

By default it connects to a local Midnight Node at `ws://localhost:9944` and exposes its GraphQL API at `0.0.0.0:8088`. All configuration has defaults except for the secret used to encrypt stored sensitive data which must be provided via the `APP__INFRA__SECRET` environment variable as valid hex-encoded 32 bytes.

`indexer-standalone` be configured by the following environment variables:

| Env Var | Meaning | Default |
|---|---|---|
| APP__APPLICATION__NETWORK_ID | Network ID | `undeployed` |
| APP__INFRA__STORAGE__CNN_URL | SQlite connection URL | `/data/indexer.sqlite` |
| APP__INFRA__NODE__URL | WebSocket Endpoint of Midnight Node | `ws://localhost:9944` |
| APP__INFRA__API__PORT | Port of the GraphQL API | `8088` |
| APP__INFRA__SECRET | Hex-encoded 32-byte secret to encrypt stored sensitive data | - |
| APP__INFRA__SPO_NODE__BLOCKFROST_ID | Blockfrost API key (required, must be non-empty; needed by spo-indexer which is included in the standalone binary). Use any non-empty placeholder if you are not exercising SPO features. | - |

For the full set of configuration options see [config.yaml](indexer-standalone/config.yaml).

### Cloud Mode

The Chain Indexer, Indexer API, Wallet Indexer and SPO Indexer can be run as separate executables, interacting with a PostgreSQL database and a NATS messaging system. Running PostgreSQL and NATS is out of scope of this document. The respective Docker images are:
- [`chain-indexer`](https://hub.docker.com/r/midnightntwrk/chain-indexer)
- [`indexer-api`](https://hub.docker.com/r/midnightntwrk/indexer-api)
- [`wallet-indexer`](https://hub.docker.com/r/midnightntwrk/wallet-indexer)
- [`spo-indexer`](https://hub.docker.com/r/midnightntwrk/spo-indexer)

#### `chain-indexer` Configuration

| Env Var | Meaning | Default |
|---|---|---|
| APP__APPLICATION__NETWORK_ID | Network ID | `undeployed` |
| APP__INFRA__STORAGE__HOST | PostgreSQL host | `localhost` |
| APP__INFRA__STORAGE__PORT | PostgreSQL port | `5432` |
| APP__INFRA__STORAGE__DBNAME | PostgreSQL database name | `indexer` |
| APP__INFRA__STORAGE__USER | PostgreSQL database user | `indexer` |
| APP__INFRA__PUB_SUB__URL | NATS URL | `localhost:4222` |
| APP__INFRA__PUB_SUB__USERNAME | NATS username | `indexer` |
| APP__INFRA__NODE__URL | WebSocket Endpoint of Midnight Node | `ws://localhost:9944` |

For the full set of configuration options see [config.yaml](chain-indexer/config.yaml).

#### `indexer-api` Configuration

| Env Var | Meaning | Default |
|---|---|---|
| APP__APPLICATION__NETWORK_ID | Network ID | `undeployed` |
| APP__INFRA__STORAGE__HOST | PostgreSQL host | `localhost` |
| APP__INFRA__STORAGE__PORT | PostgreSQL port | `5432` |
| APP__INFRA__STORAGE__DBNAME | PostgreSQL database name | `indexer` |
| APP__INFRA__STORAGE__USER | PostgreSQL database user | `indexer` |
| APP__INFRA__PUB_SUB__URL | NATS URL | `localhost:4222` |
| APP__INFRA__PUB_SUB__USERNAME | NATS username | `indexer` |
| APP__INFRA__API__PORT | Port of the GraphQL API | `8088` |
| APP__INFRA__SECRET | Hex-encoded 32-byte secret to encrypt stored sensitive data | - |

For the full set of configuration options see [config.yaml](indexer-api/config.yaml).

#### `wallet-indexer` Configuration

| Env Var | Meaning | Default |
|---|---|---|
| APP__APPLICATION__NETWORK_ID | Network ID | `undeployed` |
| APP__INFRA__STORAGE__HOST | PostgreSQL host | `localhost` |
| APP__INFRA__STORAGE__PORT | PostgreSQL port | `5432` |
| APP__INFRA__STORAGE__DBNAME | PostgreSQL database name | `indexer` |
| APP__INFRA__STORAGE__USER | PostgreSQL database user | `indexer` |
| APP__INFRA__PUB_SUB__URL | NATS URL | `localhost:4222` |
| APP__INFRA__PUB_SUB__USERNAME | NATS username | `indexer` |
| APP__INFRA__SECRET | Hex-encoded 32-byte secret to encrypt stored sensitive data | - |

For the full set of configuration options see [config.yaml](wallet-indexer/config.yaml).

#### `spo-indexer` Configuration
| Env Var | Meaning | Default |
|---|---|---|
| APP__APPLICATION__NETWORK_ID | Network ID | `undeployed` |
| APP__INFRA__STORAGE__HOST | PostgreSQL host | `localhost` |
| APP__INFRA__STORAGE__PORT | PostgreSQL port | `5432` |
| APP__INFRA__STORAGE__DBNAME | PostgreSQL database name | `indexer` |
| APP__INFRA__STORAGE__USER | PostgreSQL database user | `indexer` |
| APP__INFRA__NODE__URL | WebSocket Endpoint of Midnight Node | `ws://localhost:9944` |
| APP__INFRA__NODE__BLOCKFROST_ID | Blockfrost API key for Cardano stake queries | - |

The Blockfrost API key is required for stake distribution queries against Cardano. Without it, spo-indexer will still index committee membership, epoch data, and D-parameter changes, but stake-related data (pool metadata, stake distribution) will not be available.

For the full set of configuration options see [config.yaml](spo-indexer/config.yaml).

### Sourcing Secrets from Files

Any of the above environment variables can instead be sourced from a file by appending `_FILE` to
its name and setting that to the file's path. This suits secrets mounted as Kubernetes Secret or
Docker Secret volumes:

```bash
export APP__INFRA__SECRET_FILE=/run/secrets/indexer-secret
export APP__INFRA__STORAGE__PASSWORD_FILE=/run/secrets/postgres-password
```

Surrounding whitespace is trimmed, so a trailing newline is harmless. Files larger than 64 KiB are
rejected. An unreadable, oversized or non-regular file aborts startup rather than leaving the value
silently unset.

The value is read directly into the configuration and is **never** written back into the process
environment, so it does not appear in `/proc/<pid>/environ` — which is the point of mounting a
secret as a file. Setting `APP__X` directly takes precedence over `APP__X_FILE`, in which case the
file is not read at all.

Passing a secret directly as an environment variable still works, but the Indexer prints a warning
at startup naming (never printing) each such variable. That warning is advice for the next deploy
rather than this one: `/proc/<pid>/environ` reflects the environment the process was *exec'd* with,
so once a secret is there it cannot be removed for the lifetime of the process — `unsetenv` hides it
from the program while leaving it visible in `/proc`.

`docker-compose.yaml` is the worked example: it takes the same host variables developers already
export, hands them to the containers as Docker secrets under `/run/secrets`, and points the
matching `APP__*_FILE` variables at them. No developer setup changes.

### Running Locally

For development, you can use Docker Compose or run components manually:

#### Using Docker Compose

A `docker-compose.yaml` file is provided that defines services for the Indexer components as well as for dependencies like `postgres`, `nats`, and `node`. The latter are particularly interesting when running Indexer components "manually".

#### Manual Startup

The justfile defines recipes for each Indexer component to start it alongside its dependencies. E.g. run the Chain Indexer like this:

```bash
just run-chain-indexer
```

## Development Setup

### Requirements

- **Rust**: The latest stable version, see `rust-toolchain.toml`.
- **Cargo**: Comes with Rust.
- **cargo-nextest**: For running tests, see [nextest](https://github.com/nextest-rs/nextest/).
- **Just**: A command-runner used for tasks. Install from [just](https://github.com/casey/just).
- **direnv**: For a reproducible development environment.
- **Docker**: For integration tests and running services locally.
- **subxt-cli**: For fetching metadata from the node, see [subxt](https://github.com/paritytech/subxt?tab=readme-ov-file#downloading-metadata-from-a-substrate-node). Note: Version used must match version in Cargo.toml.
- **sql-formatter**: For formatting SQL files. Install from [sql-formatter](https://sql-formatter-org.github.io/sql-formatter/) and either integrate into your editor or run `find . -type f -name "*.sql" -exec sh -c 'sql-formatter -o "$1" "$1"' _ {} \;`.

### Environment Variables

As we allow zero secrets in the git repository, you need to define a couple of environment variables for build (tests) and runtime (tests). Notice that the values are just used locally for testing and can be chosen arbitrarily; `APP__INFRA__SECRET` must be a hex-encoded 32-byte value.

It is recommended to provide these environment variables via a `~/.midnight-indexer.envrc` or `./.envrc.local` file which is sourced by the `.envrc` file:

```bash
export APP__INFRA__STORAGE__PASSWORD=postgres
export APP__INFRA__PUB_SUB__PASSWORD=nats
# nosemgrep: generic.secrets.security.detected-generic-secret.detected-generic-secret
export APP__INFRA__SECRET=303132333435363738393031323334353637383930313233343536373839303132
# export APP__INFRA__NODE__BLOCKFROST_ID=<your-blockfrost-api-key>  # required for spo-indexer (cloud mode)
# export APP__INFRA__SPO_NODE__BLOCKFROST_ID=<your-blockfrost-api-key>  # required for indexer-standalone (any non-empty value works if not testing SPO features)
```

### Benchmarks

Criterion micro-benchmarks live under `indexer-common/benches/` and `chain-indexer/benches/` and require the `standalone` feature (simpler sqlite-backed `ledger_db` bootstrap than the cloud path). Run the whole suite with:

```bash
just bench
```

Or per crate:

```bash
cargo bench -p indexer-common --features standalone
cargo bench -p chain-indexer --features standalone
```

`chain-indexer/benches/apply_transaction.rs` reads `indexer-common/tests/genesis_state.raw` plus the same `tx_*.raw` fixtures the integration tests use. Both are regenerated together by `just generate-txs` (requires Docker); re-run it after any ledger/node version bump.

### Required Configuration for private Repositories

You may need access to private Midnight repositories and container registries. To achieve this:

#### GitHub Personal Access Token (PAT)

Create a classic PAT with:

- `repo` (all)
- `read:packages`
- `read:org`

#### Docker Authentication

```bash
echo $GITHUB_TOKEN | docker login ghcr.io -u <YOUR_GITHUB_ID> --password-stdin
```

### GPG Setup (Signed Git Commits)

All contributors are required to **cryptographically sign their Git commits** using GPG. This confirms the identity of each contributor and marks commits as verified.

#### Step 0: Prerequisites (once)

**macOS**
```bash
brew install gnupg pinentry-mac
# Optional: ensure macOS uses GUI pinentry for passphrase prompts
echo "pinentry-program $(which pinentry-mac)" > ~/.gnupg/gpg-agent.conf
killall gpg-agent || true
```

**Linux (Ubuntu/Debian)**
```bash
sudo apt-get update && sudo apt-get install -y gnupg pinentry-curses
# pinentry-curses gives a passphrase prompt in the terminal
# On desktops, you can install pinentry-gtk2 instead
```
#### Step 1: Generate a GPG key (ed25519)

```bash
gpg --quick-generate-key "Your Name <you@example.com>" ed25519 sign 2y
```
- Replace **Your name** and **<you@example.com>** with the one you use for Git.
- `sign` limits the key’s usage to signing (good hygiene).
- `2y` sets the key to expire in 2 years (you can renew later).

Already have a key? List them with:

```bash
gpg --list-secret-keys --keyid-format=long
```

Copy the key ID (the long hex after sec) for the next step—looks like ABCDEF1234567890.

#### Step 2: Tell Git to always sign

Set this globally (applies to all repositories):

```bash
git config --global user.name "Your Name"
git config --global user.email "you@example.com"

# Use OpenPGP (GPG) for signing
git config --global gpg.format openpgp

# If your key ID is ABCDEF1234567890:
git config --global user.signingkey ABCDEF1234567890

# Always sign commits and tags
git config --global commit.gpgsign true
git config --global tag.gpgsign true
```

#### Step 3: Add the GPG_TTY line to your shell config (critical)

This ensures the pinentry prompt works in your terminal.

**macOS (zsh default)**
```bash
echo 'export GPG_TTY=$(tty)' >> ~/.zshrc
source ~/.zshrc
```

**Linux (bash)**
```bash
echo 'export GPG_TTY=$(tty)' >> ~/.bashrc
source ~/.bashrc
```

#### Step 4: Upload your public key to your Git host (GitHub/GitLab)

Export your public key:
```bash
gpg --armor --export ABCDEF1234567890
```

Copy the entire block including:

```bash
-----BEGIN PGP PUBLIC KEY BLOCK-----
...
-----END PGP PUBLIC KEY BLOCK-----
```
Then upload it to your Git host:

**GitHub**: Settings → SSH and GPG keys → “New GPG key”
**GitLab**: Preferences → GPG Keys → “Add key”

> Make sure your Git email matches the email on the key and on the remote host account settings.

#### Step 5: Test it

In any repo:

```bash
git commit --allow-empty -m "test: signed commit"
git log --show-signature -1
```

You should see a “Good signature” line.
If it asks for a passphrase, that’s normal—the agent can cache it.

### LICENSE

Apache 2.0.

### SECURITY.md

Provides a brief description of the Midnight Foundation's security policy and how to properly disclose security issues.

### CONTRIBUTING.md

Provides guidelines for how people can contribute to the Midnight project.

### CODEOWNERS

Defines repository ownership rules.

### CLA Assistant

The Midnight Foundation appreciates contributions, and like many other open source projects asks contributors to sign a contributor License Agreement before accepting contributions. We use CLA assistant (https://github.com/cla-assistant/cla-assistant) to streamline the CLA signing process, enabling contributors to sign our CLAs directly within a GitHub pull request.

### Dependabot

The Midnight Foundation uses GitHub Dependabot feature to keep our projects dependencies up-to-date and address potential security vulnerabilities.

### Checkmarx

The Midnight Foundation uses Checkmarx for application security (AppSec) to identify and fix security vulnerabilities. All repositories are scanned with Checkmarx's suite of tools including: Static Application Security Testing (SAST), Infrastructure as Code (IaC), Software Composition Analysis (SCA), API Security, Container Security and Supply Chain Scans (SCS).

### Unito

Facilitates two-way data synchronization, automated workflows and streamline processes between: Jira, GitHub issues and Github project Kanban board.
