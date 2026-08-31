---
title: Transaction Block-Budget Calculator
---

Every Midnight transaction competes for two independent budgets, and only one of
them is a Substrate weight.

The one described in [Block Weights](weights.md) is Substrate's: `max_block` is
2 seconds of `ref_time`, of which the Normal dispatch class may use 75%. The
`pallet_midnight` weight of a transaction is
`ledger_gas + ConfigurableTransactionSizeWeight`, where the second term is a flat
1% of `max_block` charged to every transaction regardless of size.

The other budget is the ledger's, and it is the one that usually binds first. The
`LedgerParameters.limits.block_limits` are a five-dimensional `SyntheticCost`:

| Dimension       | What it bounds                            |
| --------------- | ----------------------------------------- |
| `read_time`     | modelled time spent reading state         |
| `compute_time`  | modelled single-threaded compute          |
| `block_usage`   | serialized transaction bytes in the block |
| `bytes_written` | net bytes persisted to disk               |
| `bytes_churned` | bytes written temporarily or overwritten  |

Each applied transaction accrues its cost into the block's running fullness, and
`prevalidate_post_block_update` rejects the transaction that would push any
dimension past its limit. The *largest* of the five fractions is what the ledger
gas — and therefore the Substrate weight — is derived from
(`scale_normalized_cost`). So the practical question for capacity work is not
"how much weight does this transaction cost" but **which dimension binds, and
which part of the transaction put it there**.

The calculator answers that per transaction.

## Producing the logs

Run any node — validator or not — with the `midnight::tx_budget` target at
`debug`:

```bash
midnight-node -lmidnight::tx_budget=debug
```

Nothing is computed when the target is off, so this is safe to leave out of
production images and to switch on for the duration of a load run. Each applied
transaction emits one JSON line — user transactions as `"k":"tx"`, the system
transactions the runtime applies itself as `"k":"sys"` — and each block emits one
more as `"k":"blk"`. System transactions accrue into the same block fullness as
user ones, so billing them too is what lets a block's fill be reconciled against
the transactions in it. A transaction costs roughly a kilobyte of log, so budget
about 350 MB per hour at 100 TPS.

The lines are emitted by the node that *applies* the transaction, which means
every node on the network produces them, and a block that is both authored and
imported locally produces its transactions twice. The report tool deduplicates on
`(parent block, transaction hash)`.

## Reading a line

```json
{"k":"tx","p":"<parent block hash>","tb":1780000000,"tx":"<tx hash>","sz":4096,
 "c":{"rt":5814000000,"ct":11940161730,"bu":8065,"bw":1786,"bc":14364},
 "s":{"rt":0.002907,"ct":0.005970,"bu":0.008065,"bw":0.035720,"bc":0.000287},
 "bind":"bw","bs":0.035720,
 "fb":[0,0,0,0,0],"fa":[5814000000,11940161730,8065,1786,14364],
 "a":[{"n":"validation.contract_call_proof","q":1,"ct":1250983791,"s":0.000625}, …]}
```

* `c` is the raw cost, `s` the same as a fraction of each block limit. Times are
  picoseconds, the rest are bytes.
* `bind` / `bs` are the dimension closest to its limit and how close — the
  transaction's real share of the block budget. `1 / bs` is how many such
  transactions fit in a block.
* `fb` / `fa` bracket the transaction within its block (fullness before and
  after, in `rt, ct, bu, bw, bc` order), so a run can be re-sequenced offline
  even when two block executions interleave in the log.
* `a` is the itemised bill. Aspects the transaction did not incur are omitted.

Block lines carry the same shape for the finished block, plus the `lim` the
shares were taken against. System-transaction lines carry the same shape as user
ones, with a single aspect named after the variant (`system.distribute_night`,
`system.cnight_generates_dust_update`, …) — the ledger prices them as one figure
per variant, so the variant is the whole itemisation.

## The aspects

| Aspect                           | Charged for                                            |
| -------------------------------- | ------------------------------------------------------ |
| `validation.baseline`            | the cost model's fixed per-transaction floor            |
| `validation.verifier_key_read`   | one state read per distinct contract entry point called |
| `validation.zswap_input_proof`   | one PLONK verification per shielded coin spent          |
| `validation.zswap_output_proof`  | one PLONK verification per shielded coin created        |
| `validation.contract_call_proof` | verifier-key load plus PLONK verification per call      |
| `validation.dust_spend_proof`    | one PLONK verification per Dust spend (i.e. per fee)    |
| `validation.signature_verify`    | unshielded, maintenance and Dust-registration signatures |
| `validation.pedersen_binding`    | the binding-commitment check, once per intent           |
| `validation.pedersen_delta`      | the balance check, once per token type per offer        |
| `validation.tx_size`             | the serialized size, against `block_usage`              |
| `validation.other`               | anything the split failed to attribute — see below      |
| `apply.guaranteed`               | applying the guaranteed section to ledger state          |
| `apply.fallible`                 | applying the fallible sections                           |

The **totals are never reconstructed**: they come from the ledger's own
`Transaction::validation_cost` and `application_cost`, so a transaction's
reported cost is exactly what the chain charged it. Only the split between
aspects is computed here, from the public cost model, and the aspects always sum
back to the total.

What that split cannot account for lands in `validation.other`. On a healthy
build it is a picosecond of rounding. A large residual means `midnight-ledger`'s
validation cost model has moved and
`ledger/src/versions/common/tx_budget_aspects.rs` needs re-syncing — the totals
stay trustworthy, the itemisation does not. The unit tests in that module assert
the residual stays under 0.1%, so a ledger bump that changes the model fails CI
rather than quietly producing a misleading breakdown.

## Reporting

```bash
python3 scripts/tx-budget-report.py node.log
python3 scripts/tx-budget-report.py run/*.log.gz --csv per-tx.csv --json summary.json
docker logs midnight-node 2>&1 | python3 scripts/tx-budget-report.py -
```

The report gives, for the run:

* the share of each block limit a transaction takes — mean, p50, p95, max — and
  the implied transactions per block;
* which dimension binds, and how often;
* where the budget goes, per aspect, per dimension;
* how full blocks actually ended up;
* the Substrate-weight ceiling alongside the ledger one, and which of the two
  binds first.

That last comparison is the point of having both numbers. A transaction whose
binding ledger share is, say, 3.6% is charged `0.036 × 2s = 72 ms` of `ref_time`
plus the flat 20 ms size weight; against the 1.5 s Normal-class budget that is
~16 transactions per block, while the ledger's own limits would have allowed ~28.
Which side is the real constraint decides whether the next capacity win is in the
ledger cost model or in the pallet's weight mapping.

The Substrate-side constants default to the values in `runtime/src/lib.rs`
(`max_block` 2 s, Normal ratio 75%, size weight 20 ms); override them with
`--max-block-weight`, `--normal-ratio` and `--tx-size-weight` when analysing a
chain configured differently.
