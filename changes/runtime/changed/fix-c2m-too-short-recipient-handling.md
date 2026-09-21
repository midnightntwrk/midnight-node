#runtime #c2m-bridge

# Fix handling of c2m-bridge transaction that have recipient address too short

In an unlikely event of Governance approving Cardano transaction with too short
Midnight recipient address, bridge would stall processing it.
Only runtime upgrade (perhaps fix present in this PR) or a manual update of bridge
data checkpoint past the invalid transaction could fix the issue.

Fixed by marking transactions with too short recipient address as Invalid.
Too long recipient was handled before.

PR: https://github.com/midnightntwrk/midnight-node/pull/2185
Issue: https://github.com/midnightntwrk/midnight-node/issues/2184
