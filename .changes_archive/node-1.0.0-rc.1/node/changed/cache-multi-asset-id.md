#node
# Cache multi_asset.id to avoid excessive joins

Add an in-memory cache for `multi_asset.id` lookups, replacing repeated `JOIN multi_asset` in
db-sync queries with a single cached lookup per (policy, name) pair. This eliminates the
`multi_asset` join from registration, deregistration, asset create/spend, and candidate token
queries, reducing query complexity and improving observation performance.

PR: https://github.com/midnightntwrk/midnight-node/pull/934
JIRA: https://shielded.atlassian.net/browse/PM-21995
