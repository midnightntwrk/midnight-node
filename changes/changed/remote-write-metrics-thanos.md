# Remote write metrics to Thanos Receive

Add remote_write metrics pusher (protobuf + snappy) with CLI flag `--prometheus-push-endpoint`, default 60s interval, and HTTP/1.1 client for reliability.

Jira: https://shielded.atlassian.net/browse/SRE-1623

PR: https://github.com/midnightntwrk/midnight-node/pull/436
