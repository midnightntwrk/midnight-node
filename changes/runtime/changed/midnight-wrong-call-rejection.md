#runtime #midnight
# Unsigned non-ledger calls to pallet-midnight are rejected with InvalidTransaction::Call

`pallet_midnight` only accepts `send_mn_transaction` as an unsigned extrinsic. Any other call
submitted unsigned was rejected with `InvalidTransaction::Custom(0)`, a code that also means
"ledger failed to deserialize the network id", so a client could not tell the two apart.

Both `validate_unsigned` and `pre_dispatch` now reject such a call with
`InvalidTransaction::Call`, Substrate's own "transaction call is not expected" variant, which
the runtime's `CheckCallFilter` already uses for disallowed calls. Clients that matched on
`Custom(0)` to detect a wrong call must match on `Call` instead; code 0 now only ever means the
ledger deserialization error.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/2247
