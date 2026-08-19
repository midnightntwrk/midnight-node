#node #ledger
# Anchored ledger tips are tagged with the block hash after import

The node watches every executed block import (including initial, gap, and
file sync — not only the post-sync import stream) and swaps each Anchored
ledger persist for a wrapper tagged with that block's header hash. The swap
is staged only; the next block's `on_finalize` flush makes it durable, so
this task never empties the write cache mid-execution. A crash before that
flush leaves the raw pin; catch-up re-swaps. The GC worker later
`release_tagged`s wrappers whose hash has left the pruning window. Transient
intra-block states still use a raw persist. Warp/fast sync still skips
hashes whose inner ledger state is not in the local DB.

Requires a resync from genesis; pre-wrapper anchored roots are not migrated.

PR: <link to PR>
