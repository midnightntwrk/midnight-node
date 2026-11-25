#toolkit #performance
# Replay reads only the blocks it needs from the fetch cache

Every command that replayed the chain first loaded all cached blocks into
memory, even when the wallet cache meant only the newest few would be replayed.
On a long, dense chain that was gigabytes of memory and a long wait before any
work started, on every run. A chain source now keeps its blocks in the fetch
cache: a warm run reads just the blocks after the cached height, and a full
replay streams the chain in windows of 10,000 blocks, so memory no longer grows
with chain length. `-v` also prints a `[perf]` breakdown of replay time by phase
with the storage arena size and process memory, at the progress heartbeat and
at the end of the replay.

PR: https://github.com/midnightntwrk/midnight-node/pull/2414
Issue: https://github.com/midnightntwrk/midnight-node/issues/2413
