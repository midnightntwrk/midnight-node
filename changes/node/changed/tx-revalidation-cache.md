#node #performance
# Replace soft tx cache with revalidation-based validation cache

Removed the soft transaction cache and the strict `VerifiedTransaction` cache,
and introduced a cache of proof receipts: the ledger state key and timestamp a
transaction last passed `well_formed` at. A cached transaction is re-checked
against a `RevalidationReference` from that state to the current one, which
skips the stateless checks (ZK proof verification included) and repeats only
those that depend on what changed. Adds cache metrics (miss, strict hit,
revalidation hit) and tests covering the full validation lifecycle.

Receipts hold no ledger allocations, so the cache no longer leaves objects in
the shared ledger storage for a block flush to write to disk, and a receipt
written by mempool validation also serves block authoring and import.

Every hit re-runs the timestamp-dependent checks at the caller's own `well_formed`
timestamp, so a transaction can no longer enter a block having only been checked
against the mempool's skewed block context (#1924). Hits still dry-run the
guaranteed segment against the current state — `well_formed` does not check
applicability, so without it an already-applied transaction would survive in the
pool.

Receipts are never invalidated by a rejection, only evicted by capacity or when
their state can no longer be loaded. A receipt records what `well_formed` proved
at a given state and timestamp rather than a validity verdict, so a rejection
does not falsify it, and keeping rejected and applied transactions cached means a
reorg that returns one to the pool revalidates it instead of re-verifying it from
scratch.

PR: https://github.com/midnightntwrk/midnight-node/pull/744
Required for https://github.com/midnightntwrk/midnight-node/issues/1178
             https://github.com/midnightntwrk/midnight-node/issues/1159
