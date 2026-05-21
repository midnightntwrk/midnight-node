#node
# Add per-SQL-query Prometheus timing for midnight data source queries

Midnight-specific data sources (cNight observation, federated authority,
candidates) now record individual Prometheus timing histograms for each
SQL query executed against DBSync. 13 sub-query timers provide per-query
latency visibility at `:9615/metrics` under the
`midnight_data_source_query_time_elapsed` metric with `query_name` labels.

PR: https://github.com/midnightntwrk/midnight-node/pull/904
JIRA: https://shielded.atlassian.net/browse/PM-22100
