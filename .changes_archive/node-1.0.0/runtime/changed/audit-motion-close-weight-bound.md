#runtime
# Add proposal_weight_bound parameter to motion_close

The motion_close extrinsic previously declared a constant weight that did not
account for the inner call dispatched as Root when a motion is approved.
Substrate can only refund weight post-dispatch, never increase it, so the inner
call's weight was never pre-charged. This adds a proposal_weight_bound parameter
following the pallet_collective::close pattern, ensuring the declared weight
includes the inner call's weight upfront. The extrinsic is also made
DispatchClass::Operational to match the other governance extrinsics. Toolkit and
upgrader are updated to pass the new parameter.

PR: https://github.com/midnightntwrk/midnight-node/pull/1032
Ticket: https://shielded.atlassian.net/browse/PM-22326
