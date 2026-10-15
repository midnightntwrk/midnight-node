#local-env #fork
# Snapshot forks: give boot-node the converted chain data

After `mock-authorities convert`, non-validator services (`extraServices`, e.g. boot-node) get a copy of the first validator's converted data dir, minus its keystore. Previously boot-node kept the original authority set, rejected the synthetic handover block, and the fork never finalized.

PR: https://github.com/midnightntwrk/midnight-node/pull/2308
Issue: https://github.com/midnightntwrk/midnight-node/issues/2305
