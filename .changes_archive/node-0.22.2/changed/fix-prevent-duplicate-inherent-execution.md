#node
# Prevent duplicate inherent execution within same block

Add InherentExecutedThisBlock guard to cnight-observation and federated-authority-observation
pallets to ensure inherents can only execute once per block. This prevents potential issues
from multiple inherent calls being processed in a single block.

PR: https://github.com/midnightntwrk/midnight-node/pull/575
JIRA: https://shielded.atlassian.net/browse/PM-21649