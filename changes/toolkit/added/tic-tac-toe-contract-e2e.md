#toolkit

# Add tic-tac-toe contract e2e test

Ports the tic-tac-toe contract from midnight-contracts and plays a full
two-player game through the compile/prove/submit/on-chain-verify pipeline:
deploy with the X and O player keys, alternate `make_move` between the two
players (each move proven with that player's `--coin-public` while fees are
paid by `FUNDING_SEED`), then assert the outcome via the `verify_game_state`
and `verify_winner` circuits.

Exercises Map- and Counter-backed on-chain state, `ownPublicKey()`-based turn
validation across two wallets, and a witness-free contract config.

PR: https://github.com/midnightntwrk/midnight-node/pull/1940
