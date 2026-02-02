# Fix ephemeral env connection timeouts

Ephemeral env port forwarding connections were timing out. This adds a "watchdog" that reconnects them when disconnected. This requires running the relevant "stop" command for the given environment, so that these are removed once done testing.

PR: https://github.com/midnightntwrk/midnight-node/pull/583
JIRA: https://shielded.atlassian.net/browse/PM-21588
