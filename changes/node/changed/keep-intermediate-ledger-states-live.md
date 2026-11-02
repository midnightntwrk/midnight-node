#storage #performance
# Keep intra-block ledger states live instead of persisting them

Intra-block ledger states are no longer persisted, and only the post-block tip
is. The others are held by a `LedgerStatesExt` externalities extension that
the node registers once per runtime-API instance, so they live exactly as long
as the block's execution. The post-block update drops them all before the
storage flush. Extensions are not transactional, so a FRAME storage rollback
after a successful ledger apply still finds the state it reverts to.
Previously every host call re-deserialized the ledger working set, roughly
three times per transaction; it is now materialised about once per block. DB
writes per block are unchanged.

Also fixes `has_ledger_state` panicking, instead of returning `false`, for an
unresolvable state key.

No storage-layout, host-ABI or runtime change.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1442
PR: https://github.com/midnightntwrk/midnight-node/pull/2050
