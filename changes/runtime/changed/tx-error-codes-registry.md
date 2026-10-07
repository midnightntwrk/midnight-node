#runtime #throttle #ledger
# Shared registry for InvalidTransaction::Custom codes; throttle rejections return code 254

Adds `midnight-primitives-tx-error-codes`, a dependency-free `no_std` crate that is the one
shared list of `InvalidTransaction::Custom(u8)` codes. The ledger adapter owns `0..=LEDGER_LAST`
(250) and `HOST_API` (255); node-side codes are the discriminants of the `NodeTxCode` enum,
allocated downward from 255, and the exhaustive match in `From<NodeTxCode> for u8` checks at
compile time that each one is outside the ledger's allocation. `#[repr(u8)]` makes two node
codes with one value a compile error. `midnight-node-ledger` takes its `HostApiError` code from
the list and tests that every code it produces satisfies `is_ledger_code`.

`pallet_throttle::CheckThrottle` now rejects a signed transaction that would take its
account over the per-window byte or transaction limit with
`InvalidTransaction::Custom(254)` (`NodeTxCode::ThrottleLimitExceeded`) instead of
`InvalidTransaction::ExhaustsResources`, so a throttled account can tell the throttle apart
from the per-transaction weight limit. Clients that matched on `ExhaustsResources` to detect
throttling must match on `Custom(254)` instead.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/2247
