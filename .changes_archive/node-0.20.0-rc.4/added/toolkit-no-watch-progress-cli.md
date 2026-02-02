#toolkit
# Add `--no-watch-progress` CLI option

Adds a `--no-watch-progress` CLI flag to the toolkit to skip waiting for transaction finalization when sending transactions. This can speed up batch sending when finalization confirmation is not needed.

Note: Using this option when sending batches may cause transaction errors, as subsequent transactions may be submitted before their dependencies are finalized.

The existing `MN_DONT_WATCH_PROGRESS` environment variable is preserved as a fallback.

PR: https://github.com/midnightntwrk/midnight-node/pull/472
JIRA: https://shielded.atlassian.net/browse/PM-21220
