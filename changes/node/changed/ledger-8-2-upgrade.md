#ledger #dependencies
# Upgrade ledger 8 from 8.1.0 to 8.2.0-rc.1

Bumps the ledger-8 stack to `midnight-ledger` 8.2.0-rc.1 (picking up
`apply_guaranteed_only`, infallible `post_block_update`, split-phase execution
with deferred events, and the segment-type refactor), keeping ledger 9 as-is.

8.2.0-rc.1 is not published to crates.io, so the ledger-8 crates are sourced
directly from the `ledger-8.2.0-rc.1` git tag. Because the tag carries path
deps, ledger 8 no longer shares its companion crates with ledger 7 — each gets
a dedicated `-ledger-8` entry pinned to the tag. Per-generation storage code
now uses each ledger's own `DefaultHasher` instead of a hardcoded `sha2::Sha256`
so it tracks the git-8.2 storage-core's sha2 0.11 bump automatically, and the
hard-fork state-migration helpers serialize-round-trip across the (now
distinct) L7/L8/L9 storage arenas instead of reinterpreting arena keys in place.

PR: TODO
