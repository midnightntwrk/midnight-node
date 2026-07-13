#toolkit
# Fix timing of Dust spends causing invalid proofs

From @tkerber

> Dust should not be vulnerable to contention if properly created. My suspicion is that what’s going on is that the wallet’s view is lagging, but it declares spend times as if the view is up-to-date. My suspicion is concretely
>
> - The wallet sees state s0 at time t1 (let’s call the block time of s0  t0)
> - Nodes are in the process of finalizing state s1 at time t1.
> - The wallet creates a dust spend, referencing a spend time of t1
> - When the node receives this, it attempts to validate it against s1, and fails
>
> If my suspicion is correct, the fix would be to use the finalized block time in dust spend creation instead of wall clock time.

The previous behaviour (using current time as DUST time) is still available via the `--dust-warp` flag. Using this flag, we are able to reproduce [PM-20611](https://shielded.atlassian.net/browse/PM-20611)

Fixes: https://shielded.atlassian.net/browse/PM-20611
PR: https://github.com/midnightntwrk/midnight-node/pull/286
