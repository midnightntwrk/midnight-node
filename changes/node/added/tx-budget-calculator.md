#performance #observability
# Add a per-transaction block-budget calculator

Adds a `midnight::tx_budget` log target that itemises what each applied
transaction takes from the block budget, for analysis after a load run on a
deployed network.

The ledger bounds a block by a five-dimensional `SyntheticCost` (read time,
compute time, block usage, bytes written, bytes churned), and the largest of
those fractions is what a transaction's Substrate weight is derived from. Run a
node with `-lmidnight::tx_budget=debug` and it emits one JSON line per applied
transaction — raw cost, share of each limit, which dimension binds, the block's
fullness before and after — and one per block. System transactions are billed the
same way, so a block's fill reconciles against the transactions in it. Nothing is
computed when the target is off.

Each line carries an itemised bill: Zswap input/output proofs, contract-call
proofs, Dust-spend proofs, signature checks, verifier-key reads, Pedersen
checks, transaction size, and guaranteed/fallible state application. Totals come
from the ledger's own `validation_cost` / `application_cost` and are therefore
exactly what the chain charged; only the split is reconstructed, and whatever it
cannot attribute is reported explicitly as `validation.other` (asserted in tests
to stay under 0.1%, so a ledger cost-model change fails CI rather than quietly
producing a misleading breakdown).

`scripts/tx-budget-report.py` turns a run's logs into a report: share of each
limit per transaction, which dimension binds and how often, where the budget
goes per aspect, how full blocks ended up, and the Substrate-weight ceiling
alongside the ledger one — including which of the two binds first. See
`docs/tx-budget.md`.

PR:
