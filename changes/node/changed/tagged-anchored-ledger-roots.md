#node #ledger
# Anchored ledger tips are tagged with the block number at persist time

`on_finalize` and genesis persist each Anchored ledger tip as a wrapper
tagged with the block number (passed from `on_finalize(block)`; genesis
is `0`). The tag and the persist commit in the same `flush_storage`. Warp
ledger-sync persists the recovered arena as a wrapper tagged with the
warp-target number in the snapshot import flush. Historical WASM that
still calls host ABI v1/v2 peeks `System::Number` instead. The GC worker
later `release_tagged`s wrappers whose height has left the pruning
window. Forks at the same height share a tag and are released together.
Transient intra-block states still use a raw persist.

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: https://github.com/midnightntwrk/midnight-node/pull/1991
