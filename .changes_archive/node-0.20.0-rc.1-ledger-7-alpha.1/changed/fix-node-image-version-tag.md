# Fix node image version tags to preserve semver pre-release suffix

Docker image version tags for `node-image` and `node-benchmarks-image` now
correctly preserve semver pre-release suffixes (e.g., `-rc.1`). Previously,
version `0.19.0-rc.1` was incorrectly tagged as `0.19.0`.

PR: https://github.com/midnightntwrk/midnight-node/pull/388
Ticket: https://shielded.atlassian.net/browse/PM-20907
