#node
# Add Prometheus Remote Write support

Adds the ability to push metrics via Prometheus Remote Write protocol for environments where pull-based scraping is not feasible. Supports Thanos Receive, Cortex, Mimir, and other remote write compatible endpoints.

New environment variables:
- `PROMETHEUS_PUSH_ENDPOINT` - Remote write URL (e.g., https://thanos.example.com/api/v1/receive)
- `PROMETHEUS_PUSH_INTERVAL_SECS` - Push interval in seconds (default: 15)
- `PROMETHEUS_PUSH_JOB_NAME` - Job name label (default: midnight-node)

Uses protobuf encoding with snappy compression as per the Prometheus Remote Write specification.

Requires `--prometheus-port` to be enabled for metrics collection.

PR: https://github.com/midnightntwrk/midnight-node/pull/436
Ticket: https://shielded.atlassian.net/browse/SRE-1623
