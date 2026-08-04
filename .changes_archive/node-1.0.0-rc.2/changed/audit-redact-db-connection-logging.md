#node
# Redact database connection details from error logs

Database connection error messages no longer include the host, port, or database name at error level. Full connection details are available at debug log level for authorized troubleshooting.

PR: https://github.com/midnightntwrk/midnight-node/pull/1067
JIRA: https://shielded.atlassian.net/browse/PM-19904
