#storage #performance
# Keep intra-block ledger states live instead of persisting them

Intra-block ledger states are no longer persisted. Each is held in a
process-global keep-alive cache until the next call releases it, and only the
post-block tip is persisted. Previously every host call re-deserialized the
ledger working set, roughly three times per transaction; it is now materialised
about once per block. DB writes per block are unchanged.

New gauge `ledger_state_cache_size{cache_type="transient"}`
(`transient_ledger_8` for ledger 8), sampled at each block's storage flush. A non-zero
`transient` reading there is a leaked intra-block state.

Also fixes `has_ledger_state` panicking, instead of returning `false`, for an
unresolvable state key.

No storage-layout, host-ABI or runtime change.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1442
PR: https://github.com/midnightntwrk/midnight-node/pull/2050
