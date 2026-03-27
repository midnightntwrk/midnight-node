#node
# Enforce TLS certificate and hostname validation for DB connections

Set ssl_mode to PgSslMode::VerifyFull in get_connection and reject
insecure SSL modes (Prefer, Disable) to prevent plaintext database
transport and unauthenticated TLS connections.

PR: https://github.com/midnightntwrk/midnight-node/pull/TBD
JIRA: https://shielded.atlassian.net/browse/PM-22023
