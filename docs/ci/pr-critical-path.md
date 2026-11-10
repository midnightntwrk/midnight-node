# Preparing local integration tests before node images finish

Local E2E compilation does not require a running node or a node image. CI starts
`+local-env-ci-prepare` alongside the image build, then Local Environment Tests
inherits that same Earthly target after its image dependencies finish. The shared
BuildKit cache supplies the compiled test binary without exporting an archive or
publishing a preparation image.

## Scope and compatibility

This changes compilation scheduling only. Local test source, thread counts,
faucet settings, node configuration and Toolkit jobs retain their existing behavior.
The local suite still starts a fresh stack, checks finality and faucet funding,
runs every enabled test and checks post-suite liveness.

The preparation job uses the existing self-hosted pool for trusted PRs in the
Midnight and Shielded organizations. External forks skip preparation and compile
inline on their existing GitHub-hosted runner. Missing or evicted cache entries
also rebuild inline. Separate preparation is useful only when both jobs share
BuildKit cache; isolated hosted jobs would need explicit artifact transfer.

The run ID and attempt invalidate the local test execution layer without changing
the preparation layers. Existing workflow tree-cache guards still apply. A cached
or skipped test result does not demonstrate that a changed test suite passed.

The preparation target has no dependency on image jobs or their outputs. Changes
to local test concurrency compose with this target because the same tests are
compiled and executed. Toolkit compilation and execution remain independent.

## Validation

Run actionlint and the repository's applicable pre-commit checks before pushing.
In CI, verify that preparation compiles successfully, Local Environment Tests
reuses the compilation layer, the complete suite executes, and liveness passes.
Check the latest head's actual test logs, not just its overall status.

Measure workflow start through the last required check, including queueing and
image dependencies. Record compilation, stack startup and test execution separately.
Parallel compilation can increase shared-host contention; reducing one job's time
does not guarantee that the whole PR finishes sooner. Compare equivalent product
code and account for host load and cache warmth.

The earlier combined candidate at `2ad2d0481` demonstrated compilation reuse:
preparation compiled in 6m12s and the later Cargo check took 2.84s. That run also
included separate test-concurrency changes, so its 13m07s local-job duration is
not a benchmark of this compilation-only PR. Validate this narrowed head separately.

## Cost and rollback

No runner infrastructure is added or moved. No GHA cash saving or Karpenter cost
increase is claimed; the existing self-hosted rental cost stays unchanged.
Reverting the preparation job and making `local-env-ci` compile inline restores
the previous scheduling without changing tests or runner infrastructure.
