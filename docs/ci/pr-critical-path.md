# Measuring the PR critical path

Measure the time from a PR workflow starting until its last required check
finishes. Job runtime alone does not measure the delay before the PR can merge.
Record dependency completion, runner start, test execution and check completion
separately. Advisory checks and metadata-triggered reruns are separate from the
required compute path.

## Comparison procedure

1. Capture a baseline with the existing workflows and unchanged product code.
2. Run the candidate workflows against the same product code. Keep test coverage,
   test inputs, image selection and required check names intact.
3. Confirm that test steps actually ran; tree-cache hits are not performance
   evidence. Record image/build cache hits separately from test-result cache hits.
4. Compare the complete dependency path, including preparation, runner waits and
   test startup. Identify concurrent runner load and cache warmth as confounders.
5. Keep the change draft if the candidate fails, skips required work or does not
   demonstrate an improvement before merge.

The initial candidates are preparing Rust integration-test binaries alongside
node-image compilation and partitioning slow local bridge tests onto independent
chain state. Keep the existing shared-state locks within each test process.
Runner-capacity changes require separate infrastructure configuration; a node
workflow change cannot reserve runner slots across other repositories.

## Implementation and runner scope

Existing jobs keep their runner selectors. The preparation job uses the existing
self-hosted pool for trusted PRs and runs alongside node-image compilation. It
warms the exact Earthly targets later used by Local Environment Tests and Test
Toolkit. This avoids publishing build images or transferring large artifacts.
Toolkit compilation selects `--package midnight-node-toolkit` as well as the
existing Nextest execution filter, so unrelated workspace test binaries are not
compiled and carried into its test image. Validate both the selected test count
and test outcomes when changing Cargo package selection.

Preparation is skipped for external fork PRs; those retain inline compilation
on GitHub-hosted runners. Missing or evicted cache entries rebuild normally.
A shared cache is required for this particular preparation-job optimization;
isolated GitHub-hosted jobs would need explicit artifact transfer. No hosted
runner saving or new ARC migration is claimed.

Local Environment Tests uses two isolated nested Docker daemons on the existing
self-hosted runner. The bridge-recipient partition contains the approved short
recipient and invalid recipient tests; the default partition contains every
other test. Both retain the existing six-thread libtest execution and shared
state locks. The partition runner reads the actual test inventory, checks the
ignored-test set, and verifies exact filters before execution. New tests enter
the default partition automatically. Fork PRs run the complete suite on one
chain. Each chain retains startup, finality, faucet funding and post-suite
liveness checks.

The GitHub run ID and attempt invalidate Earthly's test execution layer without
invalidating the preparation layers. The existing workflow tree-cache guards
still apply; a guarded skip cannot be counted as evidence of faster execution.

## Validation and rollback

Run the partition checks without compiling the node:

```bash
python3 -m unittest discover -s scripts/tests -p 'test_local_e2e_shards.py' -v
```

CI must show that both partitions executed, their combined inventory covers the
complete suite, and neither failed. Compare complete required-check latency
against the baseline on the same product code. Account for preparation time,
queueing, node-build slowdown from concurrent compilation, and additional chain
startup work. More parallel work can increase shared-host contention.

Revert this PR to restore the original workflow. No runner infrastructure,
registry publication, branch protection or test-source change is needed.
