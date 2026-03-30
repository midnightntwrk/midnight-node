#toolkit #security
# Fix race condition in LedgerContext::update_from_tx

Hold the ledger_state mutex for the full read-modify-write cycle in update_from_tx, preventing potential lost updates under concurrent access. Addresses audit finding Issue AJ.

PR: https://github.com/midnightntwrk/midnight-node/pull/767
Ticket: https://shielded.atlassian.net/browse/PM-19905
