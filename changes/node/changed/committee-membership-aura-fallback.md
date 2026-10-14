#node #babe
# Report committee membership correctly for validators on the AURA key fallback

The `committee-membership` log matched the BABE committee only against loaded
BABE keys, falling back to AURA keys only when no BABE key was loaded. BABE
authoring falls back to the AURA key per requested public key, so a validator
whose registered BABE key is its AURA key kept authoring but was reported as
`IS NOT in the committee` once any other BABE key (e.g. the next key loaded
ahead of a rotation) was in the keystore. The check now resolves keys the same
way authoring does and notes when the AURA fallback is in use.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/2295
