#node #performance
# Replace soft tx cache with revalidation-based validation cache

Removed the soft transaction cache and introduced revalidation-based caching:
cached `VerifiedTransaction` entries are reused when ledger state changes by
revalidating against a `RevalidationReference` instead of re-running full ZK
proof verification. Adds cache metrics (miss, strict hit, revalidation hit) and
tests covering the full validation lifecycle.

An entry is only reused as-is when both the ledger state and the `well_formed`
timestamp match; a differing timestamp revalidates rather than strict-hits, so a
transaction can no longer enter a block having only been checked against the
mempool's skewed block context (#1924). Revalidation hits still dry-run the
guaranteed segment against the current state — `well_formed` does not check
applicability, so without it an already-applied transaction would survive in the
pool.

Entries are never invalidated, only evicted by capacity or TTI. An entry records
what `well_formed` proved at a given state and timestamp rather than a validity
verdict, so a rejection does not falsify it, and keeping rejected and applied
transactions cached means a reorg that returns one to the pool revalidates it
instead of re-verifying it from scratch.

PR: https://github.com/midnightntwrk/midnight-node/pull/744
Required for https://github.com/midnightntwrk/midnight-node/issues/1178
             https://github.com/midnightntwrk/midnight-node/issues/1159
