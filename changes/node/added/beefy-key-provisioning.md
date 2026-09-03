#node #keystore #beefy

# Provide BEEFY keys via seed file, and add them to the local-env validators

Added `BEEFY_SEED_FILE`, alongside the existing `AURA_SEED_FILE`,
`BABE_SEED_FILE`, `GRANDPA_SEED_FILE` and `CROSS_CHAIN_SEED_FILE`. The seed is
inserted into the keystore under the `beef` key type as ECDSA, like the
cross-chain key.

Validators need a BEEFY key in their keystore once beefy is part of
`SessionKeys`; the key is looked up by `(key type, public key)`, so a key
inserted only under another key type does not satisfy a BEEFY request even when
the bytes are identical.

The local-env validators (nodes 2 to 5) now carry one. The committee registers
each validator's cross-chain key as its beefy key — `beefy_pub_key` equals
`sidechain_pub_key` in `res/local/permissioned-candidates-config.json` — so the
existing cross-chain key material is reused rather than new keys generated:
keystore entries for the keystore-mounted nodes, and `seeds/beefy.seed` plus
`BEEFY_SEED_FILE` for the seed-file node.

PR: https://github.com/midnightntwrk/midnight-node/pull/2084
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
