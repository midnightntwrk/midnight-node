#audit
#client
# Filter failed events when applying transaction

The auditors pointed out that `calls_and_deploys` includes failed segments, which means we're creating events even for failed events. This PR filters failed segments out during `apply_transaction` without changing the behavior for other uses of `calls_and_deploys`

Ticket: https://shielded.atlassian.net/browse/PM-19972
PR: https://github.com/midnightntwrk/midnight-node/pull/275
