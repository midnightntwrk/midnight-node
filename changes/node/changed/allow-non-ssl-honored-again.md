#node
# Honor `allow_non_ssl` again for db-sync database connections

Since the TLS enforcement change, `allow_non_ssl` was accepted but ignored,
every db-sync Postgres connection used `PgSslMode::Require`, and a node
pointed at a database without TLS support failed to start with "server does
not support TLS". Operators reaching db-sync over an already-secured transport
(localhost, a VPN or mesh network) with no TLS on Postgres had no
configuration that allowed the node to connect.

`allow_non_ssl = true` (or `ALLOW_NON_SSL=1`) now maps to `PgSslMode::Prefer`:
TLS when the server offers it, plaintext otherwise, with a startup warning.
The secure defaults are unchanged: without the flag connections still require
TLS, `ssl_root_cert` still forces `VerifyFull` and takes precedence over
`allow_non_ssl`, and `PgSslMode::Disable` remains unreachable.

PR: https://github.com/midnightntwrk/midnight-node/pull/2122
