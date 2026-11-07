#runtime #node

# Ad `Op` variant for `ClaimBridgeTransfer`

The added `Op::ClaimBridgeTransfer` variant. `Op` is passed in only from Runtime to Node, 
so updating Node before the Runtime makes it a safe change. Decode side is updated before Encode side.
The new variant is added as last to `Op`, so existing variants SCALE encoding doesn't change.

Claim of bridge transferred amount haven't yet happened in any durable environment yet, so future
consumers of `ClaimRewards` and `ClaimBridgeTransfer` can be sure what caused these events.
This is not true for lower testnets where old runtime will encode both kinds of claims as `ClaimRewards`.

`TransactionAppliedStateRoot` is not changed, it has only one field for amounts of both kinds of claims.

`Op` / `Operation` types, and the static runtime metadata is regenerated accordingly.

PR: https://github.com/midnightntwrk/midnight-node/pull/1727
Issue: https://github.com/midnightntwrk/midnight-node/issues/1084
