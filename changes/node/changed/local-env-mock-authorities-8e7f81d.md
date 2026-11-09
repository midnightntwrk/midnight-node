#local-env #fork-testing
# Snapshot forks: default to mock-authorities `8e7f81d`

`local-environment` and the `fork-network` workflow now default to mock-authorities `8e7f81d` instead of `f6cbf91`. The newer image builds the fork block through the runtime, so its state root is derived rather than predicted (shieldedtech/midnight-node-ops#110), and adds `convert --fork-at`. `convert`'s default fork point (the finalized tip) is unchanged. Override with `MOCK_AUTHORITIES_IMAGE` as before.
