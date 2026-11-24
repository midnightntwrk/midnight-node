#node #consensus #babe #migration

# Pick the authoring engine from the best block every slot

The AURA and BABE slot workers both run for the node's life, and each authors only on a best
block whose state selects its engine. Authoring therefore follows the best chain in both
directions around the flip, without a restart.

PR: https://github.com/midnightntwrk/midnight-node/pull/2407
