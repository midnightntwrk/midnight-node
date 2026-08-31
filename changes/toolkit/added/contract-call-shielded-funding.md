#toolkit #contract-custom

# Fund shielded coins taken by a contract call

A circuit that accepts one of the caller's shielded coins (`receiveShielded`) left the zswap
offer with an output nothing paid for, so the transaction was rejected as unbalanced.
`contract-custom` now balances the offer per shielded token type and covers the difference
from `--funding-seed`, refunding the change. The balance is `outputs - inputs - mints`:
coins the contract already owns pay for themselves, transients net to zero, and
`mintShieldedToken` outputs are backed by the contract, so only the remainder is charged to
the caller. Previously only unshielded contract flows could be funded, via `--utxo-inputs`.

Also bumps `@midnight-ntwrk/compact-js{,-command,-node}` to `2.5.5-rc.8` in the
`compact-0.33.0` toolkit-js variant, so `generate-intent deploy`/`circuit` accept
struct-typed arguments such as `ShieldedCoinInfo` and generic ones such as `Maybe<T>`.
`compact-runtime` is pinned exactly, since the compiler dictates it and `^0.18.0-rc.1`
floats onto a dev build that semver orders above rc.1.

Covered by a new `dao_e2e`, which ports the DAO voting contract from midnight-contracts and
plays a full round.

PR: https://github.com/midnightntwrk/midnight-node/pull/2077
