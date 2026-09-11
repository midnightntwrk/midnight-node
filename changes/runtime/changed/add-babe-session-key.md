#runtime

# Add BABE key to session keys

This migrates runtime session keys to a new shape that includes BABE key.
Migration writes value of AURA key to BABE field.
The AURA keys are then used until real BABE keys are read from Cardano.
The signatures are valid, because wrapped node keystore is able to
fallback to AURA key, when asked for a given BABE key.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
PR: https://github.com/midnightntwrk/midnight-node/pull/2113
