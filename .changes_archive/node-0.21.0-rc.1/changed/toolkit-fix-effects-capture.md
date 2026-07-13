#toolkit
# Fix effects detection in intent

`find_effects` method was returning the last found effect ignoring the others.
This leads to transactions with multiple contract calls stored in one intent to fail.

PR: https://github.com/midnightntwrk/midnight-node/pull/573
Ticket: https://shielded.atlassian.net/browse/PM-20404
