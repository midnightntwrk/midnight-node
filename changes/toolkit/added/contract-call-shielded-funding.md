#toolkit #contract-custom

# Fund shielded coins taken by a contract call

A circuit that accepts one of the caller's shielded coins (`receiveShielded`) left the zswap
offer with an output nothing paid for, so the transaction was rejected as unbalanced.
`contract-custom` now balances the offer per shielded token type and covers the difference
from `--funding-seed`, refunding the change. The balance is `outputs - inputs - mints`:
coins the contract already owns pay for themselves, transients net to zero, and
`mintShieldedToken` outputs are backed by the contract, so only the remainder is charged to
the caller. Previously only unshielded contract flows could be funded, via `--utxo-inputs`.

Also adds a `compact-0.34.0` toolkit-js variant, pinning
`@midnight-ntwrk/compact-js{,-command,-node}` `2.5.5-rc.8` and `compact-runtime`
`0.19.0-rc.0`, and makes it the default by moving `COMPACTC_VERSION` to `0.34.0`. Struct-typed
circuit arguments such as `ShieldedCoinInfo`, and generic ones such as `Maybe<T>`, were
already expressible in Compact, but `compact-js-command` had no way to construct one from a
CLI argument; `2.5.5-rc.8` adds that, so `generate-intent deploy`/`circuit` now accept them.
`compact-0.33.0` stays on `2.5.5-rc.7`.

Covered by a new `dao_e2e`, which ports the DAO voting contract from midnight-contracts and
plays a full round.

PR: https://github.com/midnightntwrk/midnight-node/pull/2077
Issue: https://github.com/midnightntwrk/midnight-node/issues/1772
