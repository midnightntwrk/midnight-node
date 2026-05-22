#toolkit
# Replace unchecked addition in wallet seed increment with checked_add

Return result rather than  panicing. Overflow now returns an explicit error instead
of producing a colliding seed that could lead to duplicate key derivation.
Addresses Least Authority audit Issue AL.

PR: https://github.com/midnightntwrk/midnight-node/pull/1081
JIRA: https://shielded.atlassian.net/browse/PM-20017
