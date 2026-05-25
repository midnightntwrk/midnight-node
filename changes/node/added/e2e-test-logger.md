#tests
# Add per-test tracing logger to the e2e suite

Replaces ad-hoc `println!` calls across the e2e crate with a `tracing`
subscriber and adds an `e2e_test!("name")` macro that every
`#[tokio::test]` invokes to enter a span tagged with the test name.
Output now carries an uptime timestamp, level, and the test name in
front of every line, so parallel runs (`--test-threads > 1`) can be
attributed and grepped per test instead of producing interleaved soup.
The default filter is `info`; override with `E2E_LOG=...`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1564
