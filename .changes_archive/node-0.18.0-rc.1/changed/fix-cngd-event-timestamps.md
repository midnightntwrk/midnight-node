# Fix timestamps for cnight-generates-dust events

We were treating milliseconds as seconds, causing all DUST to be generated in the distant future.

PR: https://github.com/midnightntwrk/midnight-node/pull/134
Ticket: https://shielded.atlassian.net/browse/PM-20007