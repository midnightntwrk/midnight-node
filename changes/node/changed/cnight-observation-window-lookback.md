#node
# Re-anchor the cNIGHT observation window near tip

A validator that syncs from genesis kept its cNIGHT observation cache window at
full width behind the tip forever, instead of tracking the tip the way a
restarted-with-state node does — holding ~100k Cardano blocks in memory where a
restarted peer holds a few thousand.

The window now keeps only a reorg-safety margin (`cardano_security_parameter +
block_stability_margin`, the deepest a Cardano reorg can go) behind the follower.
The runtime never reads further back, and deeper re-reads fall back to the live
source, so caching more is pointless. From-genesis and restarted nodes converge
to the same window.

This removes the `cnight_observation_window_size` config knob and its default as
redundant; configs that still set it keep loading (the key is ignored).
Node-local, no consensus impact.

PR: https://github.com/midnightntwrk/midnight-node/pull/1836
Issue: https://github.com/midnightntwrk/midnight-node/issues/1835
