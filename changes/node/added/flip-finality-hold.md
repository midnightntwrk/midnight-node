#node #grandpa #babe
# Hold GRANDPA finality across the AURA→BABE flip

A new GRANDPA voting rule keeps finality at the parent of the block that
scheduled the flip until the first BABE block and one more are on the best
chain. Until then nothing from the flip window is finalized, so
`midnight-node revert` can still roll the chain back to before the flip was
scheduled; `revert` now also reverts BABE's aux data. The hold only takes
effect while more than 1/3 of GRANDPA voting weight runs a node with it.

The hold is skipped once the schedule block is finalized and lifts after
`--flip-finality-hold-max-unfinalized` unfinalized blocks (default 1200).
`--flip-finality-hold-confirmations` sets the BABE blocks required after the
first (default 1) and `--no-flip-finality-hold` disables it. The
`flip-finality-hold` log target reports when the hold arms, lifts and is
disabled.

PR: https://github.com/midnightntwrk/midnight-node/pull/2356
