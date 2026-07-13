#node

# Keystore wrapper for AURA to BABE migration

Added `AuraToBabeMigrationKeystore` and wired into services.
This keystore falls back to AURA keys if BABE keys are requested but not found.
Operations that are not on hot path log warning when BABE keys are missing,
to help spotting the misconfiguration.

The added wrapper will not solve misconfiguration, when on-chain BABE key differes from keystore BABE/AURA key.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1825
PR: https://github.com/midnightntwrk/midnight-node/pull/1827
