# Shorter sessions

1200 slots / 2 hour session length => 300 slots / 30 mins session length.

We switched to a large session length to reduce instability in the network where one / two bad validators were getting chosen for the whole session. Increasing the session from 60 slots to 1200 worked well, but I believe we can get similar stability with smaller sessions. This PR will reduce the list of authorities from 1200 to 300.

PR: https://github.com/midnightntwrk/midnight-node/pull/122
JIRA: https://shielded.atlassian.net/browse/PM-17638
