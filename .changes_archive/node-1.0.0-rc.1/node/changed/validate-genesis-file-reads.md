#audit
# Validate genesis file type and size before reading

Reject symlinks, non-regular files, and oversized files (>10 MB) before
reading genesis and configuration files in the cfg module. Addresses
Least Authority audit Issue AI (unbounded reads in Cfg::load_spec).

PR: https://github.com/midnightntwrk/midnight-node/pull/832
JIRA: https://shielded.atlassian.net/browse/PM-19964
