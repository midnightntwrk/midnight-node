#node #consensus #babe #migration

# Record a BABE chain weight for AURA blocks of the migration era

AURA blocks executed by a runtime with `pallet-consensus-engine` are imported with a BABE chain
weight of 0, on the import queue and on the AURA authoring path, so BABE's fork choice can compare
blocks of both engines. Blocks in the block gap are skipped.

PR: https://github.com/midnightntwrk/midnight-node/pull/2407
