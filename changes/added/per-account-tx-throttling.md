#runtime
# Per-account signed transaction throttling

Adds a new `pallet-throttle` with a `CheckThrottle` transaction extension that limits per-account byte usage within a rolling block window. Each account is allowed up to 10 MB of transaction data per day (14400 blocks at 6s/block). Usage resets automatically once the window expires.

- New pallet at `pallets/throttle/` with `AccountUsage` storage tracking `(bytes_used, window_start_block)` per account
- `CheckThrottle<T>` implements `TransactionExtension`: `validate()` rejects transactions exceeding the limit, `prepare()` persists updated usage
- Unsigned/inherent transactions bypass the throttle
- 25 unit tests covering accumulation, window expiry, boundary conditions, multi-account isolation, and overflow safety

PR: https://github.com/midnightntwrk/midnight-node/pull/770
Ticket: https://shielded.atlassian.net/browse/PM-21204
