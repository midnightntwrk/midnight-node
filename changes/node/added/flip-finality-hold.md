#node #grandpa #babe

# Hold GRANDPA finality across the AURA→BABE flip

A new GRANDPA voting rule keeps finality at the parent of the block that
scheduled the flip until the first BABE block and one more are on the best
chain. Until then nothing from the flip window is finalized, so
`midnight-node revert` can still roll the chain back to before the flip was
scheduled. The `flip-finality-hold` log target reports when the hold arms,
lifts and is disabled.

PR: https://github.com/midnightntwrk/midnight-node/pull/2356
