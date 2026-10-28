# Toolkit Architecture

## Overview

The toolkit is a transaction generator and testing tool for Midnight. It fetches chain state, builds transactions against a local ledger context, and submits them to a node.

```
                                          User
                                           │
                                           ▼
                                ┌──────────────────────┐
                                │   CLI                │
                                │   cli.rs, commands/  │
                                └──────────────────────┘
                                           │
            ┌──────────────────┬───────────┴───────────┬────────────────────┐
            ▼                  ▼                       ▼                    ▼
       TxGenerator          toolkit_js              Genesis         Standalone Commands
    (tx_generator/mod.rs)  (toolkit_js/mod.rs)  (genesis_generator.rs)  (commands/)
            │                  │                 Latest Ledger only              │
            │                  ▼                                            ▼
            │         Node.js child process                            show-wallet
            │         toolkit.js -> compact.js                         show-address
            │           -> compact-runtime                             dust-balance
            │         Deploy | Circuit | Maintain                      show-transaction
            │                                                          root-call
            ▼                                                          runtime-upgrade
 ┌───────────────────────────────────────────┐                         ...
 │  Source (source.rs)                       │
 │  GetTxsFromFile (.mn)  GetTxsFromUrl      │
 └───────────────────────────────────────────┘
            │                          │
       from file                  from node
            │                          │
            │                          ▼
            │          ┌──────────────────────────────────┐
            │          │  Fetcher (fetcher.rs, fetcher/)  │
            │          │  Parallel block fetch + extract  │
            │          │  Runtime dispatch (runtimes.rs)  │
            │          │  Storage: InMemory|ReDB|Postgres │
            │          └──────────────────────────────────┘
            │                          │
            └──────────┬───────────────┘
                       │
                       ▼  SourceTransactions
 ┌───────────────────────────────────────────┐       ┌──────────────────────────────────────┐
 │  Builder (tx_generator/builder/)          │       │  Ledger Wallet Cache                 │
 │  ForkAwareLedgerContext (Ledger8|Ledger9) │◄ ─── ►│  wallet_state_cache.rs               │
 │  Dual-compiled common/ builders           │       │  MN_LEDGER_CACHE_DB                  │
 │  Prover: Local | Remote (remote_prover.rs)│       │  Snapshots + per-wallet state        │
 └───────────────────────────────────────────┘       └──────────────────────────────────────┘
                       │
                       ▼  SerializedTxBatches
 ┌───────────────────────────────────────────┐
 │  Destination (destination.rs)             │
 │  SendTxsToFile (.mn)   SendTxsToUrl       │
 └───────────────────────────────────────────┘
                       │
                       ▼
 ┌───────────────────────────────────────────┐
 │  Midnight Node (RPC :9944)                │
 │  MidnightNodeClient (client.rs)           │
 │  Rate-limited by Sender (sender.rs)       │
 └───────────────────────────────────────────┘
```

## Components

| Component | Re-implements | Description |
| --------- | ------------- | ----------- |
| Fetcher | Indexer | Parallel block fetching, extraction, and caching with pluggable storage backends |
| Builder | Wallet | Ledger context management, wallet state, proof generation |
| toolkit_js | midnight-js | Thin wrapper around compact.js (which wraps the compact-runtime) via Node.js bridge; Deploy/Circuit/Maintain |
| CLI | -- | Command dispatch with fork-aware subcommands (`commands/fork/ledger_8.rs`, `ledger_9.rs`, one module per ledger version) |
| TxGenerator | -- | Source → Builder → Destination pipeline |

## Fetcher Pipeline

The fetcher (`fetcher.rs`) orchestrates parallel block fetching and extraction via `fetch_all()`:

1. **Job Pusher** -- iterates block heights in steps of `BLOCKS_PER_JOB`, pushes fetch tasks onto a bounded channel
2. **Fetch Workers** (N) -- each with its own RPC client; checks storage cache, fetches missing blocks with exponential backoff (`fetch_task.rs`)
3. **Compute Workers** (M) -- extract block data via runtime version dispatch (`runtimes.rs`), verify hash chains, store results (`compute_task.rs`)
4. **Collector** -- gathers verified results, returns ordered blocks

Storage backends (`fetcher/fetch_storage/`): InMemory, ReDB, PostgreSQL.

Wallet state is cached across runs via `wallet_state_cache.rs`.

## Dual Compilation

The toolkit must support multiple ledger versions which have incompatible type hierarchies -- different `Transaction`, `Wallet`, `LedgerState` types, etc. Rather than abstracting over them with generics (which would require a shared trait boundary that doesn't exist), the builder code is written once in `common/` and compiled twice with different type aliases.

The `ledger-helpers` crate (`ledger/helpers/`) defines the transaction building primitives (wallet, ledger context, proof providers). It uses the same pattern -- `ledger_8` and `ledger_9` modules expose the same interface against different ledger versions.

In the toolkit builders, `ledger_8.rs` and `ledger_9.rs` (one file per supported ledger version) each re-root the `common/` directory as a module, aliasing the correct ledger version before the module declarations:

```
  builders/ledger_8.rs                    builders/ledger_9.rs
  ┌────────────────────────────────┐      ┌────────────────────────────────┐
  │ #[path = "common"]             │      │ #[path = "common"]             │
  │ pub mod inner {                │      │ pub mod inner {                │
  │   use ...::ledger_8            │      │   use ...::ledger_9            │
  │     as ledger_helpers_local;   │      │     as ledger_helpers_local;   │
  │   mod batches;                 │      │   mod batches;                 │
  │   mod contract_deploy;         │      │   mod contract_deploy;         │
  │   ...                          │      │   ...                          │
  │ }                              │      │ }                              │
  └────────────────────────────────┘      └────────────────────────────────┘
                    │                                   │
                    └──────────┬────────────────────────┘
                               ▼
                    builders/common/batches.rs
                    (references ledger_helpers_local)
```

The same pattern is used in `commands/fork/` for fork-aware read-only commands.

### Fork Transitions

`replay_blocks()` (`tx_generator/builder/mod.rs`) splits fetched blocks by ledger version and replays each run against the matching `ForkAwareLedgerContext` variant. The state migrations live in `ledger/helpers/src/fork/` (e.g. `fork_context_8_to_9()`).

- **No mixed Ledger 9 replay yet** -- a chain containing Ledger 9 blocks alongside earlier-version blocks trips an assertion in `replay_blocks()`
- **Wallet cache is Ledger 9 only** -- snapshots and per-wallet state are saved only once the context is on Ledger 9
