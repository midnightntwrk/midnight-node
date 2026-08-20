# Toolkit TypeScript Wrapper

## Purpose
- Provide a clean TypeScript API to call Midnight Toolkit from tests/services.
- Be environment-aware so calls use the correct RPC URLs and network IDs.

## Commands supported/mapped
- showAddress
- showViewingKey
- getContractAddress
- deployContract
- callContract
- updateContract

## Persistent cache directories

The toolkit container writes files as root. To avoid leaving root-owned orphans in per-run directories, long-lived data is kept in shared directories that are never deleted between runs:

| Path | Scoped by | Purpose |
|---|---|---|
| `.tmp/toolkit-zk-cache/<tag>/` | Toolkit tag | ZK circuit parameters (`spend.prover`, etc.). Tag-scoped so different toolkit versions don't overwrite each other's params. |
| `.tmp/toolkit-ledger-cache/<env>/` | Target env | Ledger state snapshot. Lets the toolkit restore from a known point instead of replaying the full chain on every warmup. Invalidated automatically on chain reset (chain ID mismatch). |
| `.tmp/toolkit-postgres-data/` | — | Postgres data volume for the `toolkit-postgres` fetch cache container. Accumulates raw block data keyed by chain ID; stale chains from past env resets are flagged in the progress reporter. |

Per-run directories (`.tmp/toolkit/<env>-<randomId>/`) are only used for transaction output files and are removed on `stop()`. The container's `/out` mount is chmod'd to 777 before teardown so the host process can delete them.
