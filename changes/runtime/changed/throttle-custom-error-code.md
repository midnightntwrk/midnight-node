#runtime #throttle #governance
# Throttle rejections return their own error code

`CheckThrottle` now rejects a signed transaction that would take its account over the
per-window byte or transaction limit with `InvalidTransaction::Custom(255)`
(`pallet_throttle::THROTTLE_LIMIT_EXCEEDED`), instead of
`InvalidTransaction::ExhaustsResources`.

Previously a throttled account saw "Transaction would exhaust the block limits", the same
message as the per-transaction weight limit, so it could not tell the two apart. Clients that
matched on `ExhaustsResources` to detect throttling must match on `Custom(255)` instead. Only
signed transactions are throttled, so in practice this affects governance members.

Also corrects the `WindowSize` doc comment: the throttle window is 1 hour (600 blocks at
6 s/block), not 1 day.

PR: <link to PR>
