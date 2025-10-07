#toolkit
# Improve tx hashes logged when sending transactions

Added a `midnight_tx_hash` and `extrinsic_hash` to the logging. The Midnight Tx
hash now aligns with node logs, however the extrinsic hash does not.

Ticket: https://shielded.atlassian.net/browse/PM-19524
PR: https://github.com/midnightntwrk/midnight-node/pull/87
