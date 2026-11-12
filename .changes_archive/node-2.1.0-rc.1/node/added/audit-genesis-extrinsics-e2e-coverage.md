#audit #tests #ci
# Add e2e regression coverage for genesis_extrinsics parsing

Adds a single-image test harness under `tests/chainspec-validation/` that
mutates a dev chainspec and starts the node binary against each variant,
plus a new "Chainspec Validation" CI job that runs it on every PR. Guards
against silent-truncation regressions in `parse_genesis_extrinsic_values`
(Least Authority audit Issue Y, fixed in #952). Covers three malformed
inputs (non-string, invalid hex, audit report's exact example) plus a
baseline boot check.

PR: https://github.com/midnightntwrk/midnight-node/pull/1516
Closes: https://github.com/shieldedtech/shielded-qa/issues/31
