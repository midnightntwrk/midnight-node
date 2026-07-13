#ledger #hardfork
# Fix storage initialization in the case of a rollback

Before, our storage de-init & init is performed during storage migration when updating to a new ledger version.

This works as long as the block does not rollback - in the case of a rollback, substrate would restore the storage root at the time before migration, but would not reverse the ledger storage de-init & init calls - this would put the nodes into a broken state, fixed by a restart.

This change move the ledger storage (re)initialization to the beginning of each block - and runs it only if the initialized ledger storage version != the requested one.

PR: https://github.com/midnightntwrk/midnight-node/pull/586
Ticket: https://shielded.atlassian.net/browse/PM-21682
