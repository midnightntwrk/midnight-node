#node #runtime #toolkit

# Pin ledger 9.1.0.0-rc.5 per crate instead of by the workspace tag

The `ledger-9.1.0.0-rc.5` tag is mutable and was re-pointed after PR #2096
locked it, so `Cargo.lock` named content the tag no longer did. Repinned to the
per-crate rc.5 tags (issue #2114); same source, no node code changes.

Two entries the published patchset omits, without which resolution fails:
`midnight-ledger-static` (crates.io stops at 9.0.0) and `midnight-zkir-v3`
(`publish = false`, activated by `test-utilities`).

PR: https://github.com/midnightntwrk/midnight-node/pull/2134
Issue: https://github.com/midnightntwrk/midnight-node/issues/2114
