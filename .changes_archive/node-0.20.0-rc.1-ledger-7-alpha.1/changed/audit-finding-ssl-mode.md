#audit
#node
# Require SSL for PostgreSQL database connections

PostgreSQL connections now require SSL/TLS by default to protect data in transit.

**Changes:**
- Default SSL mode changed from `Prefer` to `Require`
- Added `ALLOW_NON_SSL` environment variable to optionally allow non-SSL connections for development
- Production environments (qanet, testnet-02) explicitly disable non-SSL fallback

**Security Impact:**
- Connections will fail if the PostgreSQL server does not support SSL
- Operators must ensure their PostgreSQL instances have SSL certificates configured

PR: https://github.com/midnightntwrk/midnight-node/pull/199
Ticket: https://shielded.atlassian.net/browse/PM-19924
