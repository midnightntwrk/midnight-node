#ledger #cnight-observation #logging
# Log invalid DUST public keys in cNIGHT events at debug level

The `construct_cnight_generates_dust_event` host function no longer logs
`Error deserializing: DustPublicKey ... out of range for Fr` at error level. Owner
keys come from user-controlled Cardano registration datums, so invalid keys are
expected input; the failure is now logged at debug. The returned error is unchanged.

Because this runs natively, it also quiets the error during sync from genesis, where
historical blocks execute older runtimes.

PR: https://github.com/midnightntwrk/midnight-node/pull/2220
Issue: https://github.com/midnightntwrk/midnight-node/issues/1819
