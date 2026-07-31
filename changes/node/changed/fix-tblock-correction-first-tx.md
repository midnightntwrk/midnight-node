# Narrow the tblock correction to the first transaction of a block

The `tblock_correction` shipped in #1932 was applied too broadly: every historical transaction
was verified at `block_context.tblock + offset`. Preview stalled at #128536, unable to import
#128537, whose transaction has an intent TTL of `1784987084` — two slots earlier than the
`1784987088` that the correction produced.

The offset was measured from the wrong base. During mempool ingress the node verifies a
transaction at `ParentTimestamp + slot_duration * (1 + MaxSkippedSlots)`, and the including
block's own timestamp is already one slot past the parent's, so adding the offset to it
double-counts a slot. The correction is now measured from the parent block's timestamp
(`BlockContext::last_block_time`), reproducing exactly the timestamp that the producing node's
warm strict-cache entry was verified at — `1784987082` for the block above, which imports.

The correction is also now limited to the *first* ledger transaction of a block (no user or
system transaction applied yet, so the ledger state is still the parent's post-block state).
That is the only position where the producing node's strict cache could have hit, so it is the
only position where a transaction can have been verified ahead of its block's timestamp. It is
no longer applied on the mempool path at all, where `validate_unsigned` already skews the block
context it passes to the ledger.

The config values are unchanged, but `tblock_correction_offset` is now relative to the parent
block's timestamp and must equal `slot_duration_secs * (1 + MaxSkippedSlots)` (`6 * 2 = 12`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1964
Issue: https://github.com/midnightntwrk/midnight-node/issues/1924
