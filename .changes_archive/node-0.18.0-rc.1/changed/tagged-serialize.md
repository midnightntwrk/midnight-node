# Tagged Serialize Buffer Size

Initialize the buffer with the correct size, which includes the tag size. This is not a security concern (vectors resize automatically), but it does avoid a small re-allocation.

Ticket: https://shielded.atlassian.net/browse/PM-19969
PR: https://github.com/midnightntwrk/midnight-node/pull/120
