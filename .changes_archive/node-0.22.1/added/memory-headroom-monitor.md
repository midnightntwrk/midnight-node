#node
# Memory headroom monitor

Adds a memory monitor service that periodically checks available system memory and triggers graceful shutdown before the OOM killer strikes. On Linux, detects cgroup v2/v1 limits (for Docker/K8s) or falls back to /proc/meminfo (bare metal). Disabled by default (memory_threshold=0); set memory_threshold to a MiB value to enable.

PR: https://github.com/midnightntwrk/midnight-node/pull/771
JIRA: https://shielded.atlassian.net/browse/PM-22043
