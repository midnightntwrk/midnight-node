#node #keystore #babe

# Log the keystore's BABE public keys on startup

`BabeKeyProbe::log_babe_public_keys` now runs once in `new_partial`, logging
every BABE public key found in the node's keystore (or a warning when none are
present). Operators preparing for the AURA-to-BABE flip can now confirm which
BABE key(s), if any, a node will offer for authoring without waiting for the
periodic `midnight_babe_key_registered` readiness probe.

The plain keystore is used, not `AuraToBabeMigrationKeystore` — that wrapper
answers BABE queries with the AURA key as a fallback and would report every
node as key-ready regardless of whether a dedicated BABE key was inserted.

The reason to do so is to let node operators what the node resolves itself.

The local-env's authority node (midnight-node-3) now also provisions a BABE
seed via `BABE_SEED_FILE`, so the new log line is exercised there too.

PR: https://github.com/midnightntwrk/midnight-node/pull/2328
Issue: https://github.com/midnightntwrk/midnight-node/issues/1831
