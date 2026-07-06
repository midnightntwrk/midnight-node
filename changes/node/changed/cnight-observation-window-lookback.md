#node
# Shrink the cNIGHT observation sliding-window default so from-genesis syncs stay narrow

Lower `DEFAULT_WINDOW_SIZE` for the cNIGHT observation sliding window from
100 000 to 10 000 Cardano blocks.

`plan_refresh` trims the window to `follower - window_size` during steady
operation, so `window_size` is really a reorg-safety *lookback* behind the
follower, not a general cache size. A validator that syncs **from genesis**
advances the follower contiguously the whole way, so it never hits the narrow
"jump" re-anchor that a restart-with-state node takes — it stays in the
contiguous branch and keeps `window_size` blocks behind the follower forever.
At the old 100 000 default (100× the runtime's own observation window of 1000)
that meant a permanent ~100k-wide window: every refresh queried a Cardano range
many times wider than a healthy peer's, heavy enough to overrun the 6s slot and
starve block authoring (`Discarding proposal … block production took too long`),
silently degrading the authority until a manual restart.

The lookback is never read from cache — the runtime only queries
`[start_position, tip]`, at or ahead of the follower, so forward sync never
misses regardless of the value; the window only needs to cover backward-going
reads (Cardano reorgs, block re-imports). 10 000 is 10× the runtime window and
comfortably above Cardano's security parameter k (~2160), covering the deepest
possible reorg with headroom while cutting the steady-state window (and its
memory) ~10×. This makes a from-genesis node converge to roughly the same
narrow window a restart-with-state node already gets. No consensus impact — the
window is a node-local, in-memory artifact; out-of-window reads fall back to the
live db source.

Operators can still tune this per node via `cnight_observation_window_size`
(env `CNIGHT_OBSERVATION_WINDOW_SIZE`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1836
Issue: https://github.com/midnightntwrk/midnight-node/issues/1835
