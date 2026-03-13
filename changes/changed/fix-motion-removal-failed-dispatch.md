#audit #runtime
# Fix motion removal on failed dispatch in federated-authority pallet

The `motion_close` extrinsic previously propagated the dispatch error via
`motion_result?`, causing Substrate's transactional storage layer to roll
back `motion_remove` when the dispatched call failed. Approved-but-failed
motions became permanently stuck in on-chain storage with no recovery path.
The fix removes dispatch error propagation so `motion_close` always succeeds
once the motion is approved, with the dispatch outcome captured in the
`MotionDispatched` event. Also removes three unused error variants
(`MotionTooEarlyToClose`, `MotionAlreadyExists`, `MotionExpired`).

PR: https://github.com/midnightntwrk/midnight-node/pull/938
Ticket: https://shielded.atlassian.net/browse/PM-22085
