#toolkit
# Fix sender reporting `FAILED_TO_FINALIZE` for txs whose including block finalized

After a tx reached a best block, `wait_for_finalized` only matched
`InFinalizedBlock` on the `submit_and_watch` subscription and silently
swallowed terminal pool events (`Invalid`/`Dropped`/`Error`),
subscription errors, and stream end — then reported
`FAILED_TO_FINALIZE` without checking whether the already-known
including block finalized. When the submit node's tx pool kills the
watcher after inclusion (e.g. a stale pool copy of an already-included
tx failing pre-dispatch with `IntentAlreadyExists` at the node's own
authoring slot), a landed, finalizing transaction was reported as a
finality failure. Downstream tooling then retried the same spend from
cached state, producing false `DustDoubleSpend` / `NullifierAlreadyPresent`
rejections.

- Terminal subscription events are logged the moment they arrive, with
  their reason.
- On any watcher death (terminal event, stream end, or timeout), the
  sender falls back to polling the node directly for whether the
  `InBestBlock` block finalized, via the new reorg-safe
  `MidnightNodeClient::is_block_finalized` (at or below finalized
  height and canonical at its height).
- Fallback-confirmed txs log `FINALIZED_VIA_FALLBACK` with a `reason`
  field (instead of plain `FINALIZED`), so pool watcher deaths remain
  countable per URL.
- The fallback gets a minimum 10s polling window even when the watcher
  dies near the end of the 60s finality budget.
- Sibling fix in `wait_for_best_block`: `InFinalizedBlock` is now a
  success arm (skipping the finality wait) instead of being recorded as
  `last_status` and eventually reported as `FAILED_TO_REACH_BEST_BLOCK`
  if the subscription coalesces straight to finalization.
- `SenderError::FailedToFinalize` and the `FAILED_TO_FINALIZE` log line
  now carry the underlying reason.

PR: https://github.com/midnightntwrk/midnight-node/pull/XXXX
