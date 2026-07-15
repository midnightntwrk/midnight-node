#node #ledger
# Dispatch genesis bootstrap on the genesis blob's ledger version

Genesis storage init and root derivation previously hardcoded the newest
ledger, so a node could not bootstrap a chain whose immutable genesis was
serialized with an older ledger (e.g. live mainnet, `ledger-state[v13]`).
The new `genesis_version` module reads the blob's serialization tag and
routes `get_root` / paritydb genesis init to the matching compiled-in ledger,
newest first (ledgers 7 and 8 share the `v13` wire tag, so `v13` blobs route
to ledger 8; `v18` to ledger 9). This is not state migration - each chain
keeps its own ledger version; only the forced-newest assumption at the
genesis boundary is gone.

Regression test pins detection for every shipped genesis blob (mainnet -> L8,
undeployed -> L9, garbage -> unrecognised). The startup E2E is now a parallel
matrix - one leg boots the stock ledger-9 genesis, the other a ledger-8 (`v13`)
undeployed genesis fixture - each asserting finality advances. Each leg is its
own required status check ("Startup E2E in dev mode (ledger-N)").

PR: https://github.com/midnightntwrk/midnight-node/pull/1869
