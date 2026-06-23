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

The acceptance-envelope multiplier becomes runtime state, mirroring
`CardanoTxCapacityPerBlock`: a `UtxoPerTxOverestimate` storage value (default
`DEFAULT_UTXO_PER_TX_OVERESTIMATE = 64`, defined in
`midnight-primitives-cnight-observation` so the node-side genesis tool and tests
share one baseline) read by the node IDP via the new
`CNightObservationApi::get_utxo_per_tx_overestimate` (API v3) at `parent_hash`.
Both factors of the envelope (`tx_capacity * multiplier`) are now sourced from
the runtime, so the author's truncation cap and the runtime's `process_tokens`
bound stay identical across upgrades instead of relying on a constant compiled
into both binaries. The node only reads it on the v2 derivation path, which is
gated on a `spec_version` that ships in the same runtime as the v3 API, so a
pre-v3 runtime is never queried for it. Default value is 64, byte-identical to
the prior path on benign history, so finalized blocks replay unchanged.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
