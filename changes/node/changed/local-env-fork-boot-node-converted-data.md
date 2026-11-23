#local-env #fork
# Snapshot forks: convert the snapshot once, then replicate; bump mock-authorities

The restore now runs `mock-authorities convert --node-dir` once on the extracted snapshot and replicates the result into every data dir, boot-node included. Previously `--data-dir` converted only the validator dirs, so boot-node kept the original authority set, rejected the fork block, and the fork never finalized.

The default mock-authorities image (local-env and the `fork-network` workflow) moves from `4d8d772` to `f6cbf91`, whose fork block references a mock main-chain hash and which supports `--node-dir`. With the old image, 2.x nodes rejected every new block on the fork as regressing.

PR: https://github.com/midnightntwrk/midnight-node/pull/2308
Issue: https://github.com/midnightntwrk/midnight-node/issues/2305
Issue: https://github.com/midnightntwrk/midnight-node/issues/2307
