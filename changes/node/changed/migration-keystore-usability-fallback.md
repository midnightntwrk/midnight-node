#node #consensus

# Migration keystore falls back on key usability, not BABE presence

`AuraToBabeMigrationKeystore` sign/VRF paths now fall back to AURA when the
*requested* BABE public is missing, even if some other BABE key is present in
the keystore. Previously only an empty BABE key list triggered fallback, so a
present-but-mismatched BABE key let secondary-plain slot claims succeed via
`has_keys` and then fail at seal signing — a silent authoring miss.

PR: https://github.com/midnightntwrk/midnight-node/pull/1954
Issue: https://github.com/midnightntwrk/midnight-node/issues/1825
