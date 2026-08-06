# Profiling Midnight transaction processing

The node emits phase-level timing for every Midnight transaction it touches, on
its own log target so it can be switched on without the rest of the ledger's
debug output:

```bash
midnight-node ... -l midnight::tx_timing=debug
```

Nothing is formatted or allocated while the target is disabled, and the
block-import wrapper short-circuits entirely, so leaving the code in place costs
effectively nothing.

Two kinds of line come out: one per ledger host call (`op=apply_tx`,
`op=validate_tx`, …) and one per imported block (`op=block_import`). Both are
flat `key=value`, which is what makes them worth having over ad-hoc `⏱️` traces —
they aggregate with `awk` instead of by eye.

## What gets measured

The pre-block path for one transaction crosses the ledger host API three times.
Each crossing is a separate span, and a transaction that is rejected midway
still logs (with `outcome=err`), showing how much time was burned before the
rejection.

| Span | Host function | When |
| --- | --- | --- |
| `validate_tx` | `validate_transaction` | Mempool admission and revalidation (`validate_unsigned`) |
| `pre_dispatch` | `validate_guaranteed_execution` | Dispatch-time check, run as part of block execution — authoring *and* import/sync |
| `apply_tx` | `apply_transaction` | Executing the tx as part of a block — authoring *and* import/sync |
| `apply_system_tx` | `apply_system_transaction` | System transactions |
| `post_block_update` | `post_block_update`, `apply_post_block_update` | End-of-block ledger transition |

Phases within a span (`<name>_us`, in call order):

| Phase | Meaning |
| --- | --- |
| `deserialize_us` | Tagged deserialization of the tx blob |
| `load_state_us` | Ledger state load (arena, possibly a parity-db read) |
| `state_hash_us` | Hashing state for the strict validation cache key |
| `proof_cache_hit_us` / `proof_cache_lookup_us` | Strict-cache lookup for an existing `VerifiedTransaction` |
| `proof_verify_us` | `well_formed()` — **ZK proof verification**, normally the dominant cost on a cache miss |
| `tx_context_us` | Building the `TransactionContext` |
| `guaranteed_dry_run_us` | Dry run of the guaranteed segment |
| `tx_cost_us` | Cost-model evaluation and block-fullness prevalidation |
| `ledger_apply_us` | `LedgerState::apply` — the state transition, including Impact execution of contract calls |
| `arena_alloc_us`, `unshielded_utxos_us`, `serialize_state_root_us`, `tx_operations_us` | Bookkeeping around the apply |
| `cache_key_us`, `soft_cache_lookup_us`, `proof_verify_setup_us`, `proof_cache_insert_us`, `tx_cost_and_details_us` | Smaller steps, listed so that the phases sum to `total_us` |
| `persist_us` | Writing the new state into the arena / to disk |

Useful fields: `tx` (hash), `size` (bytes), `tx_type`, `proof=hit|miss` (whether
proof verification was skipped), `soft_cache=hit|miss`, `ops`, `utxos_created`,
`utxos_spent`, `outcome=ok|err`.

Example (line wrapped here, one line in the log):

```
op=apply_tx outcome=ok total_us=48213 size=54321 tx=3f9a… proof=miss tx_type=standard
  deserialize_us=1204 load_state_us=93 state_hash_us=41 proof_cache_lookup_us=8
  proof_verify_setup_us=210 proof_verify_us=41880 proof_cache_insert_us=57
  tx_context_us=12 tx_cost_us=180 ledger_apply_us=3204 arena_alloc_us=90
  unshielded_utxos_us=210 serialize_state_root_us=390 tx_operations_us=12 persist_us=622
```

Read that as: 87% of this transaction was ZK proof verification, because it
reached `apply_transaction` with a cold strict cache.

**Which span pays for proof verification depends on the path.** The `Bare`
extrinsic path runs `ValidateUnsigned::pre_dispatch` as the transaction is
dispatched, so on a **syncing/importing** node the `well_formed()` cost normally
lands in `pre_dispatch` (`proof=miss`) and the following `apply_tx` is a
`proof=hit`. On an **authoring** node the mempool has usually already verified
the transaction, so `validate_tx` pays it and the rest hit the cache. Summing
`proof_verify_us` across spans — rather than looking at `apply_tx` alone — is
what gives the real cost per transaction.

## Where the block's time goes

`op=block_import` closes the loop on "how much of the node's time is Midnight
transaction processing". It brackets the whole import — WASM execution, weight
accounting, state root, database commit — and attributes the ledger's share of
it:

```
op=block_import outcome=ok number=1234 hash=0x… extrinsics=12 total_ms=421.310
  ledger_ms=380.102 ledger_pct=90.2 mn_txs=8 system_txs=1
  apply_tx_ms=150.300 pre_dispatch_ms=225.800 post_block_update_ms=4.002
  deserialize_ms=9.100 deserialize_pct=2.2 ... proof_verify_ms=210.400 proof_verify_pct=50.0
  ledger_apply_ms=140.200 ledger_apply_pct=33.3 persist_ms=12.900 persist_pct=3.1
  validate_tx_ms=0.000 validate_tx_count=0
```

`ledger_pct` is the answer: everything else in `total_ms` is Substrate
machinery. `ledger_ms` is the sum of the block-execution ops — `apply_tx`,
`pre_dispatch`, `apply_system_tx`, `post_block_update` — each of which is also
broken out. `validate_tx_ms` (mempool) is *not* included.

Two caveats:

- The counters behind these numbers are process-wide, and only the wall-clock
  window is per-block. On an **authoring** node, ledger work from a concurrent
  proposal or from mempool validation running on another thread lands in the same
  window and inflates `ledger_ms`. A non-authoring node that is importing or
  syncing does all of its ledger work on the import path, so its numbers are
  clean; `validate_tx_ms` being non-zero is the signal that something else was
  running alongside.
- The wrapper sits on the import queue, so blocks this node *authors* never reach
  it — they are imported by the authorship task with state already computed. For
  the authoring side, use the per-transaction spans plus Substrate's own
  `🎁 Prepared block for proposing at #N` timing as the denominator.

## Sync as a repeatable benchmark

Block import is the same machinery as block production minus the proposer, so
syncing a fixed chain with these logs on is a repeatable profile: point a fresh
node at a snapshot, sync N blocks, aggregate the lines. Same input, same work,
directly comparable across code changes.

```bash
midnight-node ... -l midnight::tx_timing=debug 2>&1 | tee sync.log
```

## Aggregating

Mean and share per phase across all applied transactions:

```bash
grep 'op=apply_tx' sync.log \
  | tr ' ' '\n' | grep '_us=' \
  | awk -F'[=]' '{sum[$1]+=$2; n[$1]++} END {for (k in sum) printf "%-28s %10.1f us avg  %12.0f us total\n", k, sum[k]/n[k], sum[k]}' \
  | sort -k4 -nr
```

Proof-verification share of block import, per block:

```bash
grep 'op=block_import' sync.log \
  | sed -E 's/.*number=([0-9]+).*total_ms=([0-9.]+).*ledger_pct=([0-9.]+).*proof_verify_pct=([0-9.]+).*/\1 \2 \3 \4/' \
  | awk '{printf "block %-8s total %8.1fms  ledger %5.1f%%  proofs %5.1f%%\n", $1, $2, $3, $4}'
```

Total proof verification, wherever it was paid, versus everything else:

```bash
awk '/op=(apply_tx|pre_dispatch|validate_tx) /{
       for (i=1;i<=NF;i++) { split($i,kv,"="); k[kv[1]]=kv[2] }
       total[k["op"]] += k["total_us"]; proofs += k["proof_verify_us"]; delete k
     }
     END { for (op in total) printf "%-14s %12.0f us\n", op, total[op];
           printf "%-14s %12.0f us\n", "proof_verify", proofs }' sync.log
```

Cache-miss rate per span — if the same transaction misses in more than one span,
proof verification is being paid more than once:

```bash
for op in validate_tx pre_dispatch apply_tx; do
  printf '%-13s hit=%s miss=%s\n' "$op" \
    "$(grep -c "op=$op .*proof=hit" sync.log)" \
    "$(grep -c "op=$op .*proof=miss" sync.log)"
done
```

## Relationship to the Prometheus metrics

`ledger_txs_processing_time`, `ledger_txs_validating_time` and the
`ledger_tx_validation_cache_*` counters still report the same per-op totals and
are the right thing for dashboards and long-running networks. The timing logs
are for the question those cannot answer — *which phase inside the op* — and for
one-off profiling runs where you want per-transaction detail.
