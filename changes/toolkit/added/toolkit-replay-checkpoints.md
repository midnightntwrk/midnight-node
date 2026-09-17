# Toolkit: opt-in wallet-cache checkpoints during long ledger replays

`generate-txs` (and all commands sharing the `Source` args) accept
`--replay-checkpoint-interval <BLOCKS>` (env `MN_REPLAY_CHECKPOINT_INTERVAL`,
default 0 = off). When set, the cached context builder saves a ledger
snapshot + per-wallet cache entries every N replayed blocks, so an
interrupted full replay (tens of minutes on long chains) resumes from the
last checkpoint instead of starting over from genesis.

Wallets cached beyond a checkpoint boundary are withheld from that replay
chunk (`write_wallet_if_newer` additionally guards their on-disk entries
from regression), and the intermediate snapshots are collected by the
existing reference-based GC as wallet heights advance past them.

PR: https://github.com/midnightntwrk/midnight-node/pull/1968
Issue: https://github.com/midnightntwrk/midnight-node/issues/1970
