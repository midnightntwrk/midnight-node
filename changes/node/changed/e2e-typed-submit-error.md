#tests

# Unify the e2e submit helpers behind a typed SubmitError

`tests/e2e/src/api/midnight.rs` had `submit_midnight_tx` (returned the raw progress handle) and
`submit_expecting_rejection` (drove it to a rejection and returned a stringly `Box<dyn Error>`).

Add a typed `SubmitError` (`Subxt` for submission-time failures, `NotFinalized` for a watched tx
that failed pre_dispatch/execution) and collapse the two into a single `submit_midnight_tx` that
submits, waits for finalized success, and returns the finalized `ExtrinsicEvents` or a typed
`SubmitError`. Callers expecting success use `?`/`expect`; callers expecting a rejection match on
the `Err`.

`submit_expecting_rejection` had no callers and is removed. `submit_expecting_success` keeps its
best-block-inclusion semantics via a shared `submit_and_watch` helper and now also returns
`SubmitError`. The one `c2m_bridge` caller uses the returned events directly.

PR: <link>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1840
