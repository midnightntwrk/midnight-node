#node
# Enforce TLS certificate and hostname validation for DB connections

Set ssl_mode to PgSslMode::VerifyFull in get_connection and reject
insecure SSL modes (Prefer, Disable) to prevent plaintext database
transport and unauthenticated TLS connections.

Fixes: https://github.com/midnightntwrk/midnight-node/issues/1320
PR: https://github.com/midnightntwrk/midnight-node/pull/1104
JIRA: https://shielded.atlassian.net/browse/PM-22023
