# local-env epoch knobs, genesis-funded test wallets and a pipelined micro-dao e2e

`SLOTS_PER_EPOCH` (default 5) and `CARDANO_EPOCH_LENGTH` (default 60s) set the sidechain and
Cardano epoch lengths. The Cardano start time is now aligned to the Cardano epoch instead of a
fixed 30s grid. `GENESIS_SEEDS` funds the given wallets at genesis.

`scripts/tests/microdao-local-env-e2e.sh` pipelines the 14 micro-dao transactions across four
genesis-funded wallets, predicts each call's contract state instead of waiting for inclusion, and
asserts the chain ends in the predicted state. It runs with 10 slots per epoch, which halves the
blocks lost to session rotation, and fails fast when the chain stops advancing.
`scripts/tests/microdao-mock-node-e2e.sh` runs the same flow on a native `mock-proofs` node.

PR: TBD
