#node
# Add node identification labels to Prometheus remote write metrics

Add labels to pushed metrics that uniquely identify each node: peer_id, node_name,
hostname, and ip. These labels are auto-discovered from the node's configuration
and network identity, requiring no additional setup.

PR: https://github.com/midnightntwrk/midnight-node/pull/554
JIRA: https://shielded.atlassian.net/browse/PM-21604
