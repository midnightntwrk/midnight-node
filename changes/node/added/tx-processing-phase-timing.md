#observability #performance
# Add phase-level timing logs for Midnight transaction processing

Instruments the ledger host API so that every Midnight transaction reports where
its time goes, on the dedicated `midnight::tx_timing` log target
(`-l midnight::tx_timing=debug`).

One `key=value` line per ledger host call — `validate_tx` (mempool),
`pre_dispatch`, `apply_tx`, `apply_system_tx`, `post_block_update` — carrying a
delta per phase: deserialization, ledger state load, strict-cache lookup,
`well_formed()` proof verification, transaction context, guaranteed-execution dry
run, cost model, `LedgerState::apply`, UTXO bookkeeping, and persistence. Spans
are emitted on error paths too, so a rejected transaction still shows how much
time it burned before rejection. This replaces the cumulative-elapsed `⏱️` traces
in `apply_transaction` and `post_block_update`, which could not be differenced
into per-phase costs.

A `BlockImport` wrapper on the import queue adds one `op=block_import` line per
block, bracketing the whole import (WASM execution, weight accounting, state
root, database commit) and reporting the ledger's share of it as `ledger_pct` —
so "how much of the node's time is Midnight transaction processing" can be
answered per block. `ledger_ms` sums the block-execution ops, `pre_dispatch`
included: the `Bare` extrinsic path runs `ValidateUnsigned::pre_dispatch` at
dispatch time, which is where a syncing node actually pays for proof
verification. Because the wrapper sits on the import path, syncing a fixed chain
with the target enabled is a repeatable profile of block execution.

Nothing is formatted or allocated while the target is disabled. Process-wide
counters (`tx_timing::Totals`) are maintained regardless, at the cost of a few
relaxed atomic adds per transaction.

See `docs/tx-processing-profiling.md` for the field reference, the authoring-node
caveats, and aggregation one-liners.

PR: https://github.com/midnightntwrk/midnight-node/pull/2003
