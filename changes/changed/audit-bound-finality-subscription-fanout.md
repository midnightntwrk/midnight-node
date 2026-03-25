#node
# Bound GRANDPA and BEEFY finality subscription fan-out

Add per-connection and global subscription limits with bounded notification
channels for GRANDPA and BEEFY RPC handlers. Prevents resource exhaustion
from unbounded fan-out of consensus notifications.

PR: TBD
JIRA: https://shielded.atlassian.net/browse/PM-19967
