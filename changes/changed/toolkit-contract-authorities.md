#toolkit
# Fix toolkit not setting any authorities when deploying Contracts

By default, the toolkit will now use the funding seed as the authority seed. You may also explicitly pass multiple `authority-seed` arguments

PR: https://github.com/midnightntwrk/midnight-node/pull/251
Fixes: https://shielded.atlassian.net/browse/PM-20391
