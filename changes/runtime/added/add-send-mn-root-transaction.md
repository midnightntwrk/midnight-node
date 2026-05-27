#runtime
# Add root-only midnight transaction extrinsic

Added `send_mn_root_transaction` extrinsic to `pallet-midnight` (call_index 2), callable
only with Root origin (governance). It applies a midnight transaction and emits
`RootTxApplied` or `RootTxPartialSuccess` events containing the full serialized
transaction payload. This enables the toolkit indexer to extract governance-dispatched
midnight transactions purely from events, regardless of how the call was wrapped
(batch, scheduler, etc.).

Also restricted `send_mn_transaction` to unsigned origin only (`ensure_none`), so
governance actions must use the new root extrinsic instead.

PR: https://github.com/midnightntwrk/midnight-node/pull/1541
Ticket: https://shielded.atlassian.net/browse/PM-21016
