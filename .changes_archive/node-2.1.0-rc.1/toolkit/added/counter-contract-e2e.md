#toolkit #tests

# Port counter contract E2E test + compact-contract-tests workflow

Start migrating the `midnight-contracts` regression suite into `midnight-node`
(issue #1772) by reusing the existing `ToolkitTestHelper` E2E harness instead of the
separate TypeScript wrapper. Adds a `counter` contract fixture and a
`counter_increment_e2e` test driving the full compile -> prove -> submit -> verify
pipeline (deploy + `increment()`).

These tests are slow (compactc + local proving), so they run in a new workflow
(`compact-contract-tests.yml`) — nightly and on demand (`workflow_dispatch`) — rather than
per-PR CI. Cadence is gated by a `compact-contract-tests` cargo feature (enabled via
`RUN_COMPACT_CONTRACT_TESTS` through `scripts/test-toolkit.sh` and the `+test-toolkit`
Earthly target) instead of `--run-ignored`, so the workflow activates only these tests and
never sweeps in unrelated `#[ignore]d` ones.

The test is currently `#[ignore]d` for the LEDGER9-TOOLKIT-JS blocker (like bboard); once
toolkit-js v9 is vendored, the `#[ignore]` is swapped for the
`#[cfg_attr(not(feature = "compact-contract-tests"), ignore = ...)]` cadence gate.

PR: https://github.com/midnightntwrk/midnight-node/pull/1852
Issue: https://github.com/midnightntwrk/midnight-node/issues/1772
