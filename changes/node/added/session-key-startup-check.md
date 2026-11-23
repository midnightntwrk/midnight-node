#node #keystore #session-keys

# Log a validator's session keys at start-up, with an ERROR for each missing type

A node started with `--validator` now checks its keystore at start-up for a key of each
session key type (`aura`, `babe`, `gran`, `beef`) and logs one ERROR (target
`session-key-check`) per missing type, naming the seed-file variable and the
`midnight-node key insert` command that load it. The node keeps running. For each type it
holds, it logs the public keys at INFO instead, so they can be compared with the keys
registered for the validator. Non-validators log nothing.

The check reads only the plain keystore, so keys loaded by seed file and by `key insert` are
both seen, and an AURA key does not count as a BABE key. It replaces the
`BABE public key(s) in keystore: …` / `No BABE public key found in keystore` start-up lines
(target `babe-key-readiness`), which are removed.

PR: https://github.com/midnightntwrk/midnight-node/pull/2513
Issue: https://github.com/midnightntwrk/midnight-node/issues/2509
