# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

All commands default to `feature=cloud`. Override with `just feature=standalone <recipe>`.

```bash
just check          # cargo check all packages
just fmt            # format with rustfmt
just fmt-check      # check formatting (CI)
just lint           # clippy with -D warnings across all packages
just lint-fix       # clippy --fix
just test           # build executables, run nextest, verify GraphQL schema
just doc            # build docs with -D warnings
just bench          # run criterion benchmarks (indexer-common, chain-indexer; standalone feature)
just all            # license-headers + check + fmt + lint + test + doc
just all-all        # run all for both cloud and standalone features
```

Run a single test (use `--` to pass nextest filters):
```bash
cargo nextest run -p indexer-common --features cloud <test_name>
cargo nextest run -p chain-indexer --features cloud --test-threads 1
```

Run components locally (starts Docker dependencies automatically):
```bash
just run-chain-indexer
just run-wallet-indexer
just run-indexer-api
just run-indexer-standalone   # SQLite-based, no Docker dependencies
```

Regenerate the committed GraphQL schema after API changes:
```bash
just generate-indexer-api-schema
```

## Environment Setup

Copy required secrets into `~/.midnight-indexer.envrc` or `.envrc.local` (sourced by `.envrc`):
```bash
export APP__INFRA__STORAGE__PASSWORD=postgres
export APP__INFRA__PUB_SUB__PASSWORD=nats
export APP__INFRA__SECRET=...
```

All commits must be GPG-signed (`commit.gpgsign = true` is enforced by `.envrc`).

## Architecture

### Deployment Modes (Feature-Gated)

Two mutually exclusive Cargo features control the entire deployment topology at compile time:

| Feature | Database | Pub/Sub | Use Case |
|---|---|---|---|
| `cloud` | PostgreSQL | NATS | Separate microservice containers |
| `standalone` | SQLite | In-memory broadcast channels | Single binary |

Always pass `--features cloud` or `--features standalone` — there is no default feature.

### Domain / Application / Infra Layers

Each crate is structured in three layers:

- **`domain/`** — Pure Rust types and traits, no I/O. Pub/sub message types, storage traits, and business logic live here.
- **`application.rs`** — Orchestrates the indexing loop using domain traits; no direct infra dependencies.
- **`infra/`** — Feature-gated implementations of domain traits (e.g., `NatsPublisher` vs `InMemPublisher`, `PostgresPool` vs `SqlitePool`).

The `indexer-common` crate owns all shared domain types, DB pool abstractions, migrations, pub/sub traits, and crypto utilities used by the other crates.

### Pub/Sub Messaging

Components communicate via strongly-typed messages (`BlockIndexed`, `WalletIndexed`, `UnshieldedUtxoIndexed`) defined in `indexer-common/src/domain/pub_sub.rs`. Each message type carries a `const TOPIC: Topic`.

- **cloud**: NATS subjects (`pub-sub.<topic>`), JSON serialization, reconnection with retry/throttle
- **standalone**: `tokio::sync::broadcast` channels (capacity 42), background drain tasks to prevent lag errors

### Database Migrations

SQL migrations live in `indexer-common/migrations/{postgres,sqlite,sqlite-ledger-db}/`. They are embedded at compile time via `sqlx::migrate!()` macros and validated against a live DB in integration tests. Schema uses binary (`bytea`/`BLOB`) columns with version tagging for serialized Midnight ledger types (SCALE-encoded).

### GraphQL API

`indexer-api` exposes three operation types via async-graphql + axum:
- **Queries** — point-in-time reads (blocks, transactions, contract actions, wallets)
- **Mutations** — wallet session management (connect, extend, remove)
- **Subscriptions** — real-time WebSocket streams for new blocks, contract actions, and wallet events

The committed schema file `indexer-api/graphql/schema-v4.graphql` is checked by `just test` and must be updated via `just generate-indexer-api-schema` after any API change.

### Testing

- **Unit tests**: `#[tokio::test]` inline in modules
- **Integration tests**: Use testcontainers to spin up real PostgreSQL/NATS instances; live in the relevant crate's `src/` alongside the code under test
- **E2E tests**: `indexer-tests/` crate — starts all components as child processes against a running node, exercises GraphQL queries and subscriptions end-to-end

The `indexer-standalone` crate is excluded from `nextest` (it is only built as an artifact for e2e tests).

## Coding Conventions

- **`just all-all`** — run after completing a meaningful chunk of work, not after every small edit.
- **Type annotations** — omit when the compiler can infer. Use turbofish (e.g. `.collect::<Vec<_>>()`) rather than annotating the binding if a hint is needed.
- **Functional style** — prefer iterator combinators (`.map()`, `.fold()`, `.collect()`, etc.) over imperative loops with mutation.
- **Multiline SQL** — always use `indoc!{ "..." }`. Never use `\` line continuations inside string literals.
