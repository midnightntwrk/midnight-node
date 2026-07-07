#node
# Re-anchor the cNIGHT observation window near tip instead of a fixed lookback

The cNIGHT observation sliding window trims to a lookback behind the follower in
steady state. That lookback was a large fixed window, and a node syncing from
genesis advances the follower contiguously and never takes the narrow re-anchor
a restart-with-state node does — so it trailed the tip by the whole window
forever instead of tracking it. A from-genesis validator ended up holding a
~100k-block in-memory window (hundreds of MB) where a restarted peer holds a few
thousand, with the retained width depending on how the node happened to sync
rather than on the chain.

Re-anchor the window to a small reorg-safety margin (the Cardano security
parameter plus stability margin) behind the follower instead. The runtime only
ever reads at or ahead of the follower, so anything further back is never served
from cache — the only reason to keep any is a reorg, which can't run deeper than
that margin, and deeper re-reads fall back to the live source. A from-genesis
node now converges to the same near-tip window as a restart-with-state one.

The fixed-size window knob (`cnight_observation_window_size`) and its default are
removed: the reorg margin the node already tracks is the right lookback, so there
is nothing to tune. Existing configs that still carry the key keep loading (it is
ignored). No consensus impact — the window is node-local.

Note: the per-refresh db-sync pull is already incremental, so the oversized
window cost memory rather than steady-state query time; the slow refreshes
reported in the issue were during a concurrent db-sync incident, not a
window-only effect.

PR: https://github.com/midnightntwrk/midnight-node/pull/1836
Issue: https://github.com/midnightntwrk/midnight-node/issues/1835
