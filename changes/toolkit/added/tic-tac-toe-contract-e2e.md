#toolkit

# Add tic-tac-toe contract e2e test

Ports the tic-tac-toe contract from midnight-contracts and plays a full
two-player game through the compile/prove/submit/on-chain-verify pipeline:
deploy with X and O identities derived from private witness keys, alternate
`make_move` between the two players (each move proves knowledge of the current
player's secret while fees are paid by `FUNDING_SEED`), then assert the outcome
via the `verify_game_state` and `verify_winner` circuits.

Exercises Map- and Counter-backed on-chain state and private-witness turn
authorization that is independent from the public fee-paying wallet.

PR: https://github.com/midnightntwrk/midnight-node/pull/1940
