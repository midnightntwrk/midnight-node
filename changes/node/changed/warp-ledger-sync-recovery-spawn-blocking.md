#node #warp-sync
# Run warp ledger-arena recovery off the async runtime with progress logging

Warp ledger-sync recovery verified + imported the recovered arena snapshot by
calling `import_verified_ledger_snapshot` **inline** on a tokio async worker
(`LedgerSyncClient::recover`). That importer reconstructs the whole ledger arena
via the native multi-pass `Arena::deserialize_sp`, a synchronous, CPU-bound
computation that scales with arena size. On a large (load-tested) arena it runs
for minutes; run inline it monopolized a runtime worker and emitted no output,
so a warp-syncing node looked hung — best block frozen at the warp target, one
core pinned at ~100% — and could not shut down cleanly (the recovery task blocked
cooperative shutdown for the full 60s abort window).

`recover` now runs the verify+import under `tokio::task::spawn_blocking` and logs
an elapsed heartbeat every 15s while it runs, plus the total duration on success.
The CPU-bound deserialize no longer starves the async runtime (networking/RPC stay
responsive), a slow recovery is observable instead of silent, and shutdown is
prompt. Verified against live perfnet: recovery of a ~41 MB arena at a real warp
target completed on the blocking pool and released the authoring/import gate,
with async workers idle throughout.

Note: this makes recovery honest and non-blocking; it does not change the
deserialize cost itself. Making warp *complete quickly* on a large arena still
needs the upstream `midnight-storage-core` `Arena::deserialize_sp` perf work
(drop the debug re-serialize-and-compare at `arena.rs:916`, cache per-node hashes
across passes), tracked separately.

PR: <fill in after opening PR>
