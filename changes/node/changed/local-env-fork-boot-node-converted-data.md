#local-env #fork
# Snapshot forks: give boot-node the converted chain data; bump mock-authorities

After `mock-authorities convert`, non-validator services (`extraServices`, e.g. boot-node) get a copy of the first validator's converted data dir, minus its keystore. Previously boot-node kept the original authority set, rejected the synthetic handover block, and the fork never finalized.

The default mock-authorities image (local-env and the `fork-network` workflow) moves from `4d8d772` to `f6cbf91`, whose fork block references a mock main-chain hash. With the old image, 2.x nodes rejected every new block on the fork as regressing.

PR: https://github.com/midnightntwrk/midnight-node/pull/2308
Issue: https://github.com/midnightntwrk/midnight-node/issues/2305
Issue: https://github.com/midnightntwrk/midnight-node/issues/2307
