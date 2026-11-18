#toolkit
# Tidy `contract-address` `--untagged` flag handling

Hide the deprecated `--untagged` flag from `--help` (still accepted at parse
time for backward compatibility) and move its deprecation warning from
`eprintln!` to `log::warn!` so it participates in the toolkit's logging
contract (`RUST_LOG`, `--quiet`, `--verbose`). Declare `--tagged` and
`--untagged` as `conflicts_with` so passing both is a clap parse error
rather than a silent "tagged wins" outcome. The success path (no flag, or
`--tagged` alone) now emits empty stderr, matching the `show-address` sister
command's pattern.

PR: https://github.com/midnightntwrk/midnight-node/pull/1486
Issue: https://github.com/midnightntwrk/midnight-node/issues/1402
Ticket: https://shielded.atlassian.net/browse/PM-19934
