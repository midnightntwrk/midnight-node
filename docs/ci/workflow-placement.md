# CI placement and caching

PR completion time includes runner queues, compilation and test execution. Heavy
compilation stays on the persistent Hetzner pool. Short checks and image-only work
use standard GitHub-hosted runners, so they do not compete for that pool.

## Workflow decisions

| Workflow file | Decision | Reason |
| --- | --- | --- |
| `prs.yml` | Remove | The version reminder never exports its decision; it duplicates release/version policy without providing a useful check. |
| `nightly-build-check.yml` | Weekly and manual clean build on hosted | Keep an uncached build as a cache-correctness check without repeating a locked build every night. |
| `continuous-integration-checks.yml` | Move trusted Clippy to Hetzner; independent hosted formatting | Clippy compiles the workspace and can reuse scoped compiler artifacts. Formatting provides quick feedback. Forks remain hosted. |
| `continuous-integration.yml` | Retain fixture tests on Hetzner | The dual-ledger fixture crate has documented out-of-memory failures on 16 GB hosted runners. Documentation-only checks and forks remain hosted. |
| `continuous-integration.yml` | Prepare toolkit tests alongside node builds on Hetzner | Compile only the toolkit package into a nextest archive. Consume the prepared layer when images finish; rebuild inline if the shared cache is absent. |
| `continuous-integration.yml` | Run dependency check without the image dependency | Cargo-shear needs source, not a node image. Use the tool already installed in the CI image. |
| `continuous-integration.yml` | Reuse node RPC for metadata checking | Verify the new hosted image-selection and metadata-extraction paths against the PR images, without an Earthly toolchain on trusted PRs. |
| `continuous-integration.yml` | Repair indexer test caching | Existing images only skip image builds. Tests need their own authenticated success marker; publish new images only after tests pass. Restore Rust artifacts before testing. |
| `continuous-integration-test.yml` | Keep core tests on Hetzner | Compilation dominates execution. Reuse isolated compiler directories and upload the actual JUnit report instead of nonexistent coverage artifacts. |
| `main.yml` | Hosted AMD64 image-reuse path; Hetzner for builds | Probe on hosted first. Reuse immutable image digests and extract artifacts without a Rust toolchain. |
| `rebuild-metadata-bot.yml`, `rebuild-chainspec-bot.yml` | Hosted image-reuse path; Hetzner for source fallback | Permission checks and image lookup do not need a persistent build runner. Explicit unavailable versions fail rather than silently building different source. |
| `compact-contract-tests.yml` | Move scheduled Compact tests to hosted | This suite is outside the PR path. Bound compilation to two jobs and retain the full suite. |
| `nightly-run-cnight-e2e-devnet.yml` | Retain Hetzner; add funding preflight | Shared database access requires the allowlisted runner. Check faucet UTXOs before compiling; insufficient funds remain a failure. |
| `scan.yaml`, `security-audit-scan.yml` | Always refresh scans | A previously passing source tree says nothing about newly published vulnerabilities. |
| `actionlint.yml`, `feature-unification.yml` | Retain hosted; authenticate success markers | Pin the hosted OS. Actionlint also tests CI decision scripts and responds to action/script changes. |

Release ARM and deterministic WASM jobs retain their larger hosted runners; those
are distinct from the free standard runners. Benchmarks and fork-network snapshots
retain self-hosted resources. Their CPU/disk or network needs do not disappear
because they run outside the PR path. Dedicated capacity or scheduling isolation
for these jobs requires a separate runner-infrastructure change.

## Cache boundaries

Compiler caches are enabled only for trusted self-hosted jobs. The namespace
includes repository, branch, Rust toolchain, architecture and CPU model; Earthly
separates target directories by target flavor. Forks and standard hosted runners
use clean compiler directories. Cargo still checks source and dependency
fingerprints. The weekly clean build provides an independent uncached check.

Preparation shares the existing host's BuildKit daemon, as local E2E preparation
already does. It is an optimisation, not an artifact-transfer contract: a different
host or an evicted layer recompiles inline. Toolkit archive preparation does not
need a running node. Test execution still waits for the built node image.

Success markers require `TREE_GUARD_HMAC` and an exact cache-key match from a
repository branch or a same-repository PR. Fork-created entries cannot satisfy
a trusted check, even if the key is visible in logs. Missing secrets, forks, manual runs,
schedules, workflow overrides and lookup failures cause cache misses. Marker keys
include check identity and input tree; indexer markers use the submodule tree plus
the parent workflow, cache action and environment setup. Compiler reuse and test
success markers are separate: an image or compiler cache hit is not test evidence.

Documentation-only classification accepts only root README/CONTRIBUTING Markdown
and Markdown under docs/changes/.changes_archive. Everything else, an empty diff,
or a failed diff runs product checks. Required check names are preserved, with
explicit successful hosted jobs for documentation changes rather than path-filtered
workflows that leave required checks missing.

## Validation and rollback

Run actionlint, shellcheck on the changed shell scripts, and
`python3 -m unittest discover -s scripts/ci -p 'test_*.py' -v` before pushing.
Inspect current-head test counts, compiler reuse and runner placement in CI.
Compare cold and warm PR completion time including queue time; a shorter individual
job does not establish a faster PR when parallel builds compete for the same host.

Scheduled workflows and issue-comment bots are not exercised by ordinary PR checks.
Validate their hosted resource use and reuse/fallback paths before treating placement
as proven. In particular, Compact proving must fit hosted memory/disk; faucet
preflight success does not prove the multi-hour observation suite will pass.

Set `CI_CACHE_TRUSTED=false` to restore clean compiler directories. Revert a runner
expression independently if a hosted workload proves too large. Preparation can be
removed without changing test coverage: its consumer compiles inline.
