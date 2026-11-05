#toolkit #performance
# Recycle fetch connections that have been throttled

Fetch workers recycle a connection that has gone slow. Against a public RPC
endpoint a long-lived WebSocket was observed to drop from ~500 blocks/s to ~20
blocks/s after 10 to 20 minutes at full speed, with no error and on all worker
connections at once, while a freshly opened connection ran at full speed again.
A job that fetched at under a quarter of the worker's best observed rate, and
took at least 10 seconds, now makes the worker reconnect before its next job;
the rate is measured over the successful attempt only, so a retried job does
not count its failed attempt and backoff as slowness.

PR: https://github.com/midnightntwrk/midnight-node/pull/2101
Issue: https://github.com/midnightntwrk/midnight-node/issues/1937
