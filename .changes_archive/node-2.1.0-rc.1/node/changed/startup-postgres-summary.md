#node
# Log sanitized db-sync startup probe results

When `midnight-node` connects to db-sync at startup, it now runs a small startup probe that times a few existing Cardano follower reads and logs a sanitized summary of the results. The probe helps spot slow database responses without logging connection details such as host, port, SSL state, or database sizing information.

PR: https://github.com/midnightntwrk/midnight-node/pull/1411/
Issue: https://github.com/midnightntwrk/midnight-node/issues/1412
