#runtime #security
# Prevent DDoS via feeless blockspace consumption

Add pre-dispatch validation to reject transactions whose guaranteed part would fail before they are included in blocks. This prevents a DDoS attack vector where attackers could fill blocks with transactions that fail the guaranteed phase (before fees are extracted), consuming blockspace without paying fees.

PR: https://github.com/midnightntwrk/midnight-node/pull/367
Ticket: https://shielded.atlassian.net/browse/PM-20944

