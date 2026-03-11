#node
# Add per-query Prometheus metrics for midnight data source queries

Midnight-specific data sources (cNight observation, federated authority,
candidates) now record per-method Prometheus timing histograms and call
counters, matching the partner-chains instrumentation. Query performance
is visible at `:9615/metrics` with `method_name` labels.

PR: https://github.com/midnightntwrk/midnight-node/pull/904
Ticket: https://shielded.atlassian.net/browse/PM-22100
