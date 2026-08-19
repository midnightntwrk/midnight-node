#node #ledger
# Anchored ledger tips are tagged with the block hash after import

The node watches every executed block import (including initial, gap, and
file sync — not only the post-sync import stream) and swaps each Anchored
ledger persist for a wrapper tagged with that block's header hash. The
import stream is subscribed in `new_full` before the watch task is spawned,
so imports cannot miss the queue. The swap is staged only; the next block's
`on_finalize` flush makes it durable, so this task never empties the write
cache mid-execution. A crash before that flush leaves the raw pin; restart
re-tags genesis and best (they were imported before the stream existed).
Warp ledger-sync persists the recovered arena as a wrapper tagged with the
warp-target hash in the snapshot import's own flush — the target's import
notification already fired against an empty arena, so the watcher cannot
tag it. The GC worker later `release_tagged`s wrappers whose hash has left
the pruning window. Transient intra-block states still use a raw persist.
Warp/fast sync still skips *subsequent* hashes whose inner ledger state is
not in the local DB.

`StateKey` is decoded via the storage-version-gated helper (same as warp
ledger-sync / `Pallet::state_key`). Byte-sniffing would misdecode a pre-v3
key whose length is a multiple of 256 as `Transient` and skip the tip.
Pre-v3 block tips are therefore tagged on a historical full sync. Pre-v3
intra-block intermediates still leak: v1 host functions persist every
successor without unpersisting.

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: <link to PR>
