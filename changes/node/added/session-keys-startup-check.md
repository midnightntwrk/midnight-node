#node #keystore #session-keys

# Check the registered session keys against the keystore on startup

On startup a validator finds its own entry in the permissioned and registered candidate
lists on Cardano by its `crch` key and logs (target `session-keys`), for each registered
session key of the runtime's key types, whether the keystore holds it; on a runtime without
`SessionKeysGenerationApi` it logs that and skips the check. It runs alongside the BABE readiness probe and
its `midnight_babe_key_registered` metric, which are unchanged.

PR: https://github.com/midnightntwrk/midnight-node/pull/2488
