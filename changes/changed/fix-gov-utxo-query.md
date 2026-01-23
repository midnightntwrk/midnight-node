# Fix governance UTXO query by removing spend check

The query now gets the latest gov utxo before the block num. provided.

The previous query checked if that utxo had been spent, including in blocks
that are not yet stable, leading to unexpected behaviour and a potential
vulnerability.

PR: https://github.com/midnightntwrk/midnight-node/pull/529
Ticket: https://shielded.atlassian.net/browse/PM-21534
