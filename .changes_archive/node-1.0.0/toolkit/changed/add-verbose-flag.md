#toolkit
# Add --verbose and --quiet flags to toolkit CLI

Added `--verbose` / `-v` (debug level) and `--quiet` / `-q` (warn level) global
flags to the toolkit CLI. Default log level is info. Per-batch fetch log messages
have been demoted from info to debug level, reducing noise while keeping high-level
progress visible.

PR: https://github.com/midnightntwrk/midnight-node/pull/859
Ticket: https://shielded.atlassian.net/browse/PM-22220
