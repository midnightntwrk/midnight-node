#toolkit

# midnight-contracts migration

Ports dapp contracts from midnight-contracts and drives each one through the
compile/prove/submit/on-chain-verify pipeline against a running node.

**battleship** — two players wager on a hidden 3x3 board: both `start` with a
committed ship position, Blue guesses Red's square and sinks it, Red `concede`s,
and Blue `withdraw`s the merged pot plus its deposit. Covers shielded coin cells
the contract funds, merges and pays out, struct-typed circuit arguments, and a
witness the circuit writes back that later calls re-open against the commitment.

**election** — the authority allowlists two voters, opens a topic, and runs a
commit-reveal ballot through to the final phase. Covers two Merkle trees where
membership in one gates writes to the other, and an enum crossing the witness
boundary.

Each contract asserts its own outcome on-chain, so a completed run is the
assertion: battleship's `withdraw` requires the winning state, and election's
reveals require a matching commitment in the tree.

PR: <link to PR>
