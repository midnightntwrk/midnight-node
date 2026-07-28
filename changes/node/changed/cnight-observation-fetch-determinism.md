#node #runtime #security
# Make cNIGHT observation inherent generation deterministic and unskippable

The cNIGHT inherent provider used a row-count over-fetch (`tx_capacity * factor`)
as a one-shot SQL `LIMIT`, then treated "fewer than `tx_capacity` distinct txs
returned" as "range complete" and advanced the Cardano cursor to the tip —
without checking whether the query was truncated by its limit. A range holding
more matching rows than the limit across fewer txs (many UTXOs per tx) was
silently skipped, and a node that fetched more rows derived a different inherent
→ `check_inherent` rejection (fork/liveness) plus corrupted mint/burn accounting.

Fix: one deterministic path for both the in-memory cache and the db fallback.
Fetch the complete range (`bulk_pull` now reports whether its limit was hit) and
truncate whole-transaction to the runtime envelope (`tx_capacity *
UTXO_PER_TX_OVERESTIMATE`). The cursor reaches the tip only on a proven-complete
fetch; otherwise it stops at the last fully-observed tx. With no fetch-size input
left, every node derives identical inherents.

`UTXO_PER_TX_OVERESTIMATE` moves to `midnight-primitives-cnight-observation` as
the single source shared by runtime and node. Byte-identical to the prior 64x
path on benign history, so finalized blocks replay unchanged.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
