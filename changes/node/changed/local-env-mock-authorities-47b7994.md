#local-env #fork-testing
# Snapshot forks: default to mock-authorities `47b7994`

`local-environment` and the `fork-network` workflow now default to mock-authorities `47b7994` instead of `f6cbf91`. Forks of long-lived node 2.x snapshots now finalize. Older images opened the node's parity-db 0.5 database with parity-db 0.4, could not see a long chain's `grandpa_voters`, and left GRANDPA on the snapshot's authority set (shieldedtech/midnight-node-ops#127). The image also builds the fork block through the runtime (shieldedtech/midnight-node-ops#110) and adds `convert --fork-at`. `convert`'s default fork point (the finalized tip) is unchanged. Override with `MOCK_AUTHORITIES_IMAGE` as before.

PR: https://github.com/midnightntwrk/midnight-node/pull/2406
