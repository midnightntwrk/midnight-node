#toolkit
# Use cryptographically secure RNG for parent block hash fallback
Replace non-cryptographic RNG with OsRng for parent_block_hash generation in TransactionWithContext

PR: https://github.com/midnightntwrk/midnight-node/pull/878
Ticket: https://shielded.atlassian.net/browse/PM-20205
