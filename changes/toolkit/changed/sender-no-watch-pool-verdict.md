#toolkit
# `--no-watch-progress` sends now fail when the pool rejects the transaction

With `--no-watch-progress`, the sender logged `SENT` as soon as
`submit_and_watch` returned and dropped the subscription unread. A node whose
transaction pool is full does not reject the submission as an RPC error: it
accepts the subscription and delivers the rejection as the first stream event
(`Invalid`, or `Dropped`), so every such transaction was reported as sent and
the loss was invisible to the caller. Under burst load on perfnet this hid up
to 61% loss per run.

- In `--no-watch-progress` mode the sender now reads the subscription up to the
  pool's first verdict (bounded at 5s). `Invalid`/`Dropped`/`Error` fail the
  send and are logged with the existing `INVALID_TRANSACTION` /
  `DROPPED_TRANSACTION` / `TRANSACTION_ERROR` tags; any other status
  (`Validated`, `Broadcasted`, `InBestBlock`, ...) returns immediately as
  before.
- If no status arrives within the bound, or the subscription ends first, the
  send still succeeds but logs `NO_POOL_VERDICT` with the reason, so unknown
  outcomes stay countable.
- The `--no-watch-progress` help text describes the new behaviour.
