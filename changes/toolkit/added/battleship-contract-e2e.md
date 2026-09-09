#toolkit

# Add battleship contract e2e test

Ports the battleship (coracle) contract from midnight-contracts and plays a full
game on-chain: both players `start` with a wagered ship position, Blue guesses
Red's square and sinks it, Red `concede`s, and Blue `withdraw`s the merged pot
plus its deposit. `withdraw` asserts the winning state, so a completed payout is
proof the game reached it.

PR: <link to PR>
