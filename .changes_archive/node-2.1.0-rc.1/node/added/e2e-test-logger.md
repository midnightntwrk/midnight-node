#tests
# Add per-test tracing logger to the e2e suite

Replaces ad-hoc `println!` calls across the e2e crate with a `tracing`
subscriber and a `#[e2e_test]` proc-macro attribute (drop-in for
`#[tokio::test]`) that installs the subscriber and enters a span
tagged with the function name. Each log line now carries a UTC
wall-clock timestamp, uptime since the first test started, level, and
the test name, so parallel runs (`--test-threads > 1`) can be
attributed and grepped per test instead of producing interleaved
soup. The default filter is `info`; override with `E2E_LOG=...`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1564
Issue: https://github.com/shieldedtech/shielded-qa/issues/48
